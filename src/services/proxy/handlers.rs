use axum::{
    extract::{Request, State},
    http::HeaderMap,
    response::Response,
};
use super::error::ProxyError;
use super::forwarder::RequestForwarder;
use super::handler_context::RequestContext;
use super::request_logger::{RequestLogger, RequestLogBuilder};
use super::response_processor;
use super::server::ProxyState;
use crate::models::{BillingType, Provider};
use crate::services::quota::QuotaContext;

/// Main proxy handler — single entry point for /v1/* requests
pub async fn proxy_handler(
    State(state): State<ProxyState>,
    _headers: HeaderMap,
    req: Request,
) -> Response {
    // Detect error format early from the request path (before consuming request)
    let error_format = ProxyError::format_from_path(req.uri().path());

    match proxy_handler_inner(state, req).await {
        Ok(response) => response,
        Err(e) => e.into_response_with_format(error_format),
    }
}

/// Inner handler that returns Result for ergonomic `?` usage
async fn proxy_handler_inner(
    state: ProxyState,
    req: Request,
) -> Result<Response, ProxyError> {
    // Increment request count
    {
        let mut count = state.request_count.write().await;
        *count += 1;
    }
    // Distributed: increment global request counter (fire-and-forget)
    let dist = state.distributed.clone();
    tokio::spawn(async move { dist.incr_request_count().await; });

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

    // --- Auth: authenticate API key BEFORE building context (need org_id for provider loading) ---
    let auth_header = headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ProxyError::AuthError { status: 401, message: "Missing authorization token".to_string() })?;

    let auth_result = match state.authenticator.authenticate(auth_header).await {
        Ok(result) => result,
        Err(auth_err) => {
            let (status, msg) = match &auth_err {
                super::auth::AuthError::MissingToken => (401, auth_err.to_string()),
                super::auth::AuthError::RateLimited => (429, auth_err.to_string()),
                _ => (403, auth_err.to_string()),
            };
            return Err(ProxyError::AuthError { status, message: msg });
        }
    };

    // Build request context with the authenticated org_id for provider loading
    let mut ctx = RequestContext::new(
        &state,
        &headers,
        &body_bytes,
        request_path,
        request_method,
        auth_result.org_id,
    ).await?;

    // Populate auth fields into context
    // Extract client IP from X-Forwarded-For or X-Real-IP
    let client_ip = headers.get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| headers.get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()))
        .unwrap_or_else(|| "127.0.0.1".to_string());
    ctx.client_ip = Some(client_ip);

    ctx.api_key_id = Some(auth_result.key_id);
    ctx.api_key_name = Some(auth_result.key_name.clone());
    ctx.user_id = Some(auth_result.user_id);
    ctx.quota_group = auth_result.quota_group.clone();
    ctx.org_id = Some(auth_result.org_id);

    // Check API key model limits
    if let Some(ref allowed_models) = auth_result.model_limits {
        if let Some(ref model) = ctx.request_model {
            if !allowed_models.iter().any(|m| m == model) {
                return Err(ProxyError::AuthError {
                    status: 403,
                    message: format!("Model '{}' is not allowed by this API key", model),
                });
            }
        }
    }

    // Filter providers by provider_group (skip if group is "default" — matches all)
    // Also skip for pinned provider routing (explicit target overrides group restrictions)
    {
        let group = &auth_result.provider_group;
        if group != "default" && !ctx.pinned {
            let before = ctx.providers.len();
            ctx.providers.retain(|p| p.provider_group == *group);
            if ctx.providers.is_empty() {
                tracing::warn!(
                    trace_id = %ctx.trace_id,
                    "[Proxy] No providers match provider_group='{}' (had {} before filter)",
                    group, before
                );
                return Err(ProxyError::NoProvider);
            }
            tracing::info!(
                trace_id = %ctx.trace_id,
                "[Proxy] Filtered providers by group='{}': {} -> {}",
                group, before, ctx.providers.len()
            );
        }
    }

    tracing::info!(trace_id = %ctx.trace_id, model = ?ctx.request_model, path = %ctx.request_path, "[Proxy] Request started");

    // --- Quota check (rule-based, when quota_group is set) ---
    if ctx.quota_group.is_some() {
        let quota_ctx = QuotaContext {
            user_id: ctx.user_id.unwrap_or(0),
            key_id: ctx.api_key_id.unwrap_or(0),
            org_id: ctx.org_id.unwrap_or(0),
            model: ctx.request_model.clone().unwrap_or_default(),
            quota_group: ctx.quota_group.clone(),
            provider_name: None,
            is_owner: false,
        };

        // Pre-check token quota (are we already over limit?)
        match state.quota_checker.check_tokens(&quota_ctx).await {
            Ok(result) if !result.allowed => {
                return Err(ProxyError::AuthError {
                    status: 429,
                    message: format!("Quota exceeded: rule '{}', used {}/{}", result.rule_name, result.used, result.limit),
                });
            }
            Err(e) => {
                tracing::warn!(trace_id = %ctx.trace_id, "[Quota] Check failed: {}", e);
            }
            _ => {}
        }
    }

    // Create forwarder
    let forwarder = RequestForwarder::new(state.http_client.clone());

    // Forward with retry/failover
    // Error logging for each retry attempt is handled inside forward_with_retry
    let result = forwarder
        .forward_with_retry(&ctx, &body_bytes, &headers, &method, &uri, &state)
        .await?;

    let provider = result.provider;
    let response = result.response;

    // Process response (streaming / non-streaming detection + stats recording)
    let axum_response = response_processor::process_response(
        response, &ctx, &provider, &state,
    ).await;

    let axum_response = match axum_response {
        Ok(r) => r,
        Err(e) => {
            // Upstream returned error in response body (e.g. API error JSON)
            if state.config.proxy.error_log_enabled && e.should_record_error_log() {
                record_error_log(&state, &ctx, Some(&provider), &e);
            }
            return Err(e);
        }
    };

    // Inject trace_id into response headers
    let mut axum_response = axum_response;
    axum_response.headers_mut().insert(
        "x-trace-id",
        axum::http::HeaderValue::from_str(&ctx.trace_id).unwrap_or_else(|_| axum::http::HeaderValue::from_static("unknown")),
    );

    Ok(axum_response)
}

/// Record an error request to request_logs (token=0, cost=0)
/// Used for errors from process_response (upstream error in response body).
/// Per-retry errors in forward_with_retry are logged by the forwarder itself.
fn record_error_log(state: &ProxyState, ctx: &RequestContext, provider: Option<&Provider>, error: &ProxyError) {
    let duration_ms = ctx.start_time.elapsed().as_millis() as i64;
    let status_code = error.status_code();
    let error_message = format!("{:?}", error);

    let provider_id = provider.map(|p| p.id).unwrap_or(0);

    let mut log_builder = RequestLogBuilder::new(provider_id, ctx.request_time)
        .response_time(chrono::Utc::now().timestamp_millis())
        .duration_ms(duration_ms)
        .status_code(status_code)
        .error_message(error_message)
        .request_path(ctx.request_path.clone())
        .request_method(ctx.request_method.clone())
        .request_id(ctx.trace_id.clone())
        .billing_type(BillingType::Request);

    if let Some(p) = provider {
        log_builder = log_builder.provider_name(p.name.clone());
    }
    if let Some(ref model) = ctx.request_model {
        log_builder = log_builder.model(model.clone());
    }
    if let Some(ref sid) = ctx.session_id {
        log_builder = log_builder.session_id(sid.clone());
    }
    if let Some(key_id) = ctx.api_key_id {
        let key_name = ctx.api_key_name.clone().unwrap_or_default();
        log_builder = log_builder.api_key_info(key_id, key_name);
    }
    if let Some(uid) = ctx.user_id {
        log_builder = log_builder.user_id(uid);
    }
    if let Some(ref ip) = ctx.client_ip {
        log_builder = log_builder.client_ip(ip.clone());
    }

    let log = log_builder.build();

    if let Err(e) = RequestLogger::new(&state.db).log_request(&log) {
        tracing::warn!(trace_id = %ctx.trace_id, "Failed to record error log: {}", e);
    } else {
        tracing::info!(
            trace_id = %ctx.trace_id,
            "[Proxy] Recorded error request: status={}, duration={}ms",
            status_code, duration_ms,
        );
    }
}
