use axum::{
    extract::{Request, State},
    http::HeaderMap,
    response::Response,
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

    Ok(axum_response)
}
