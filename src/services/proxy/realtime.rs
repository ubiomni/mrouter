/// WebSocket proxy for /v1/realtime endpoint.
/// Handles bidirectional streaming between client and upstream provider.
///
/// Protocol:
/// 1. Client connects via WebSocket to /v1/realtime
/// 2. Server authenticates via query param or header (same API key auth)
/// 3. Server establishes upstream WebSocket to provider
/// 4. Bidirectional forwarding of messages (raw passthrough)

use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite};

use super::server::ProxyState;

/// Query parameters for the realtime endpoint
#[derive(Debug, Deserialize)]
pub struct RealtimeParams {
    /// Model to route to (used to select provider)
    pub model: Option<String>,
}

/// Idle timeout for WebSocket connections (no messages in either direction)
const WS_IDLE_TIMEOUT_SECS: u64 = 60;

/// WebSocket upgrade handler for /v1/realtime
pub async fn realtime_handler(
    ws: WebSocketUpgrade,
    State(state): State<ProxyState>,
    Query(params): Query<RealtimeParams>,
    headers: HeaderMap,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_realtime(socket, state, params, headers))
}

/// Build the upstream WebSocket URL from provider base_url
fn build_upstream_ws_url(base_url: &str, model: Option<&str>) -> Result<String, String> {
    let mut url = base_url.trim_end_matches('/').to_string();

    // Convert http(s) to ws(s)
    if url.starts_with("https://") {
        url = format!("wss://{}", &url[8..]);
    } else if url.starts_with("http://") {
        url = format!("ws://{}", &url[7..]);
    } else if !url.starts_with("wss://") && !url.starts_with("ws://") {
        url = format!("wss://{}", url);
    }

    url.push_str("/v1/realtime");

    if let Some(m) = model {
        url.push_str(&format!("?model={}", m));
    }

    Ok(url)
}

/// Main WebSocket handling logic
async fn handle_realtime(client_ws: WebSocket, state: ProxyState, params: RealtimeParams, _headers: HeaderMap) {
    let trace_id = uuid::Uuid::new_v4().to_string();
    tracing::info!(trace_id = %trace_id, model = ?params.model, "[Realtime] WebSocket connection established");

    // 1. Determine provider (use first active provider from failover queue)
    let provider = {
        let queue = state.failover_queue.read().await;
        match queue.first() {
            Some(p) => p.clone(),
            None => {
                tracing::error!(trace_id = %trace_id, "[Realtime] No active provider available");
                let _ = close_ws_with_reason(client_ws, "No active provider available").await;
                return;
            }
        }
    };

    // 2. Build upstream URL
    let upstream_url = match build_upstream_ws_url(&provider.base_url, params.model.as_deref()) {
        Ok(url) => url,
        Err(e) => {
            tracing::error!(trace_id = %trace_id, "[Realtime] Failed to build upstream URL: {}", e);
            let _ = close_ws_with_reason(client_ws, "Invalid upstream configuration").await;
            return;
        }
    };

    tracing::info!(trace_id = %trace_id, provider = %provider.name, upstream = %upstream_url, "[Realtime] Connecting to upstream");

    // 3. Build auth headers for upstream connection
    let mut request = tungstenite::http::Request::builder()
        .uri(&upstream_url)
        .header("Host", extract_host(&upstream_url).unwrap_or_default())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", tungstenite::handshake::client::generate_key());

    // Forward auth — use provider's API key
    if !provider.api_key.is_empty() {
        // Use provider.auth_header_pair() method
        if let Some((header_name, header_value)) = provider.auth_header_pair() {
            request = request.header(header_name, header_value);
        }
    }

    // Add OpenAI-specific headers if model is provided
    if params.model.is_some() {
        request = request.header("OpenAI-Beta", "realtime=v1");
    }

    let request = match request.body(()) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(trace_id = %trace_id, "[Realtime] Failed to build request: {}", e);
            let _ = close_ws_with_reason(client_ws, "Internal error").await;
            return;
        }
    };

    // 4. Connect to upstream
    let upstream_ws = match connect_async_tls_with_config(request, None, false, None).await {
        Ok((ws_stream, _response)) => ws_stream,
        Err(e) => {
            tracing::error!(trace_id = %trace_id, "[Realtime] Failed to connect to upstream: {}", e);
            let _ = close_ws_with_reason(client_ws, &format!("Upstream connection failed: {}", e)).await;
            return;
        }
    };

    tracing::info!(trace_id = %trace_id, "[Realtime] Upstream connected, starting bidirectional relay");

    // 5. Split both connections and relay bidirectionally
    relay_bidirectional(client_ws, upstream_ws, &trace_id).await;

    tracing::info!(trace_id = %trace_id, "[Realtime] WebSocket session ended");
}

/// Relay messages bidirectionally between client and upstream WebSocket
async fn relay_bidirectional(
    client_ws: WebSocket,
    upstream_ws: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    trace_id: &str,
) {
    let (mut client_tx, mut client_rx) = client_ws.split();
    let (mut upstream_tx, mut upstream_rx) = upstream_ws.split();

    let idle_timeout = std::time::Duration::from_secs(WS_IDLE_TIMEOUT_SECS);
    let trace_id_owned = trace_id.to_string();
    let trace_id_owned2 = trace_id.to_string();

    // Task: client -> upstream
    let client_to_upstream = tokio::spawn(async move {
        loop {
            match tokio::time::timeout(idle_timeout, client_rx.next()).await {
                Ok(Some(Ok(msg))) => {
                    let tung_msg = axum_msg_to_tungstenite(msg);
                    if let Some(m) = tung_msg {
                        if m.is_close() {
                            tracing::debug!(trace_id = %trace_id_owned, "[Realtime] Client sent close");
                            let _ = upstream_tx.send(m).await;
                            break;
                        }
                        if upstream_tx.send(m).await.is_err() {
                            tracing::debug!(trace_id = %trace_id_owned, "[Realtime] Upstream send failed");
                            break;
                        }
                    }
                }
                Ok(Some(Err(e))) => {
                    tracing::debug!(trace_id = %trace_id_owned, "[Realtime] Client recv error: {}", e);
                    break;
                }
                Ok(None) => {
                    tracing::debug!(trace_id = %trace_id_owned, "[Realtime] Client stream ended");
                    break;
                }
                Err(_) => {
                    tracing::info!(trace_id = %trace_id_owned, "[Realtime] Client idle timeout ({}s)", WS_IDLE_TIMEOUT_SECS);
                    break;
                }
            }
        }
    });

    // Task: upstream -> client
    let upstream_to_client = tokio::spawn(async move {
        loop {
            match tokio::time::timeout(idle_timeout, upstream_rx.next()).await {
                Ok(Some(Ok(msg))) => {
                    let axum_msg = tungstenite_msg_to_axum(msg);
                    if let Some(m) = axum_msg {
                        if matches!(&m, Message::Close(_)) {
                            tracing::debug!(trace_id = %trace_id_owned2, "[Realtime] Upstream sent close");
                            let _ = client_tx.send(m).await;
                            break;
                        }
                        if client_tx.send(m).await.is_err() {
                            tracing::debug!(trace_id = %trace_id_owned2, "[Realtime] Client send failed");
                            break;
                        }
                    }
                }
                Ok(Some(Err(e))) => {
                    tracing::debug!(trace_id = %trace_id_owned2, "[Realtime] Upstream recv error: {}", e);
                    break;
                }
                Ok(None) => {
                    tracing::debug!(trace_id = %trace_id_owned2, "[Realtime] Upstream stream ended");
                    break;
                }
                Err(_) => {
                    tracing::info!(trace_id = %trace_id_owned2, "[Realtime] Upstream idle timeout ({}s)", WS_IDLE_TIMEOUT_SECS);
                    break;
                }
            }
        }
    });

    // Wait for either task to finish, then abort the other
    tokio::select! {
        _ = client_to_upstream => {}
        _ = upstream_to_client => {}
    }
}

/// Convert axum WebSocket message to tungstenite message
fn axum_msg_to_tungstenite(msg: Message) -> Option<tungstenite::Message> {
    match msg {
        Message::Text(text) => Some(tungstenite::Message::Text(text.to_string())),
        Message::Binary(data) => Some(tungstenite::Message::Binary(data.to_vec())),
        Message::Ping(data) => Some(tungstenite::Message::Ping(data.to_vec())),
        Message::Pong(data) => Some(tungstenite::Message::Pong(data.to_vec())),
        Message::Close(frame) => {
            let tung_frame = frame.map(|f| tungstenite::protocol::CloseFrame {
                code: tungstenite::protocol::frame::coding::CloseCode::from(f.code),
                reason: f.reason.to_string().into(),
            });
            Some(tungstenite::Message::Close(tung_frame))
        }
    }
}

/// Convert tungstenite message to axum WebSocket message
fn tungstenite_msg_to_axum(msg: tungstenite::Message) -> Option<Message> {
    match msg {
        tungstenite::Message::Text(text) => Some(Message::Text(text.to_string().into())),
        tungstenite::Message::Binary(data) => Some(Message::Binary(data.into())),
        tungstenite::Message::Ping(data) => Some(Message::Ping(data.into())),
        tungstenite::Message::Pong(data) => Some(Message::Pong(data.into())),
        tungstenite::Message::Close(frame) => {
            let axum_frame = frame.map(|f| axum::extract::ws::CloseFrame {
                code: f.code.into(),
                reason: f.reason.to_string().into(),
            });
            Some(Message::Close(axum_frame))
        }
        tungstenite::Message::Frame(_) => None,
    }
}

/// Close a WebSocket with a reason message
async fn close_ws_with_reason(mut ws: WebSocket, reason: &str) -> Result<(), axum::Error> {
    let close_frame = axum::extract::ws::CloseFrame {
        code: 1011, // Internal Error
        reason: reason.to_string().into(),
    };
    let _ = ws.send(Message::Close(Some(close_frame))).await;
    Ok(())
}

/// Extract host from a URL string
fn extract_host(url: &str) -> Option<String> {
    url::Url::parse(url).ok().and_then(|u| {
        u.host_str().map(|h| {
            if let Some(port) = u.port() {
                format!("{}:{}", h, port)
            } else {
                h.to_string()
            }
        })
    })
}
