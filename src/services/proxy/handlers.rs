use axum::{
    extract::{Request, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use super::error::ProxyError;
use super::forwarder::RequestForwarder;
use super::handler_context::RequestContext;
use super::response_processor;
use super::server::ProxyState;

/// Main proxy handler — single entry point for /v1/* requests
pub async fn proxy_handler(
    State(state): State<ProxyState>,
    _headers: HeaderMap,
    req: Request,
) -> Result<Response, ProxyError> {
    // Increment request count
    {
        let mut count = state.request_count.write().await;
        *count += 1;
    }

    // Extract request info before consuming body
    let method = req.method().clone();
    let uri = req.uri().clone();
    let request_path = req.uri().path().to_string();
    let request_method = req.method().to_string();
    let headers = req.headers().clone();

    // Read body (can only be read once)
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|e| ProxyError::RequestError(e.to_string()))?;

    // Build request context
    let ctx = RequestContext::new(
        &state,
        &headers,
        &body_bytes,
        request_path,
        request_method,
    )?;

    tracing::info!(trace_id = %ctx.trace_id, model = ?ctx.request_model, path = %ctx.request_path, "[Proxy] Request started");

    // Create forwarder
    let forwarder = RequestForwarder::new(state.http_client.clone());

    // Forward with retry/failover
    let result = forwarder
        .forward_with_retry(&ctx, &body_bytes, &headers, &method, &uri, &state)
        .await?;

    let provider = result.provider;
    let response = result.response;

    // Process response (streaming / non-streaming detection + stats recording)
    let axum_response = response_processor::process_response(
        response, &ctx, &provider, &state,
    ).await?;

    // Inject trace_id into response headers
    let mut axum_response = axum_response;
    axum_response.headers_mut().insert(
        "x-trace-id",
        axum::http::HeaderValue::from_str(&ctx.trace_id).unwrap_or_else(|_| axum::http::HeaderValue::from_static("unknown")),
    );

    Ok(axum_response)
}
