//! Search Gateway — transparent proxy mode
//!
//! Authenticates the request, selects a search-category provider,
//! injects the provider's API key, and proxies the request/response
//! transparently to the upstream search API.
//!
//! Routes:
//!   POST /v1/search/*path  → upstream_base_url/*path
//!   GET  /v1/search/*path  → upstream_base_url/*path

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::sync::Arc;

use crate::database::Database;
use crate::database::dao::ProviderDao;
use crate::services::proxy::auth::{ApiKeyAuthenticator, AuthError};

/// Shared state for the search handler
#[derive(Clone)]
pub struct SearchState {
    pub db: Database,
    pub authenticator: Arc<ApiKeyAuthenticator>,
    pub http_client: reqwest::Client,
}

/// Transparent search proxy handler
/// Matches: /v1/search, /v1/search/{*path}
pub async fn search_handler(
    State(state): State<SearchState>,
    request: Request,
) -> impl IntoResponse {
    let (parts, body) = request.into_parts();
    let headers = &parts.headers;
    let method = &parts.method;
    let uri = &parts.uri;

    // 1. Authenticate
    let auth_header = headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Err(e) = state.authenticator.authenticate(auth_header).await {
        let (status, msg) = match e {
            AuthError::MissingToken => (401, "Missing authorization token"),
            AuthError::InvalidFormat => (401, "Invalid authorization format"),
            AuthError::InvalidToken => (401, "Invalid API key"),
            AuthError::KeyDisabled => (403, "API key is disabled"),
            AuthError::KeyExpired => (403, "API key has expired"),
            AuthError::QuotaExceeded => (429, "Quota exceeded"),
            AuthError::RateLimited => (429, "Rate limit exceeded"),
            _ => (500, "Authentication error"),
        };
        return (
            StatusCode::from_u16(status as u16).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            axum::Json(json!({"error": {"message": msg, "type": "auth_error"}})),
        ).into_response();
    }

    // 2. Select search provider (X-Provider-Name header or first active search provider)
    let provider_name = headers.get("x-provider-name")
        .or_else(|| headers.get("X-Provider-Name"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let provider = match select_search_provider(&state.db, provider_name.as_deref()) {
        Some(p) => p,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(json!({"error": {"message": "No search provider configured", "type": "config_error"}})),
            ).into_response();
        }
    };

    // 3. Build upstream URL: strip /v1/search prefix, forward remainder
    let path = uri.path();
    let sub_path = path.strip_prefix("/v1/search").unwrap_or("");
    let base = provider.base_url.trim_end_matches('/');
    let upstream_url = if sub_path.is_empty() || sub_path == "/" {
        base.to_string()
    } else {
        format!("{}{}", base, sub_path)
    };
    let upstream_url = if let Some(q) = uri.query() {
        format!("{}?{}", upstream_url, q)
    } else {
        upstream_url
    };

    // 4. Read body
    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, axum::Json(json!({"error": {"message": "Failed to read request body"}}))).into_response();
        }
    };

    // 4.5. For POST requests to query-based APIs (like SearchAPI):
    //      Convert JSON body to query parameters and use GET upstream
    let (effective_method, query_params, body_to_send) = if *method == Method::POST
        && provider.auth_method.starts_with("query:")
        && !body_bytes.is_empty() {
        // Parse JSON body into query params
        match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
            Ok(json_obj) => {
                if let Some(obj) = json_obj.as_object() {
                    let params: Vec<(String, String)> = obj.iter()
                        .filter_map(|(k, v)| {
                            // Convert JSON values to strings for query params
                            match v {
                                serde_json::Value::String(s) => Some((k.clone(), s.clone())),
                                serde_json::Value::Number(n) => Some((k.clone(), n.to_string())),
                                serde_json::Value::Bool(b) => Some((k.clone(), b.to_string())),
                                _ => None, // Skip arrays/objects
                            }
                        })
                        .collect();
                    (Method::GET, params, bytes::Bytes::new())
                } else {
                    (method.clone(), vec![], body_bytes.clone())
                }
            }
            Err(_) => (method.clone(), vec![], body_bytes.clone()),
        }
    } else {
        (method.clone(), vec![], body_bytes.clone())
    };

    // 5. Build upstream request
    let mut req_builder = match effective_method {
        Method::GET => state.http_client.get(&upstream_url),
        Method::POST => state.http_client.post(&upstream_url),
        Method::PUT => state.http_client.put(&upstream_url),
        Method::DELETE => state.http_client.delete(&upstream_url),
        Method::PATCH => state.http_client.patch(&upstream_url),
        _ => state.http_client.request(effective_method.clone(), &upstream_url),
    };

    // 5.5. Add query parameters from body conversion
    if !query_params.is_empty() {
        req_builder = req_builder.query(&query_params);
    }

    // 6. Forward headers (skip hop-by-hop and auth — we inject provider's key)
    let skip_headers: &[&str] = &[
        "host", "authorization", "connection", "transfer-encoding",
        "x-provider-name", "content-length",
    ];
    for (name, value) in headers.iter() {
        if !skip_headers.contains(&name.as_str()) {
            req_builder = req_builder.header(name.clone(), value.clone());
        }
    }

    // 7. Inject provider API key based on auth_method
    req_builder = apply_provider_auth(req_builder, &provider);

    // 8. Attach body (for non-GET methods)
    if effective_method != Method::GET && !body_to_send.is_empty() {
        req_builder = req_builder.body(body_to_send);
    }

    // 9. Send
    let upstream_resp = match req_builder.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Search proxy upstream error: {}", e);
            return (
                StatusCode::BAD_GATEWAY,
                axum::Json(json!({"error": {"message": format!("Upstream error: {}", e), "type": "upstream_error"}})),
            ).into_response();
        }
    };

    // 10. Stream response back transparently
    let status = StatusCode::from_u16(upstream_resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut response_headers = HeaderMap::new();
    for (name, value) in upstream_resp.headers().iter() {
        if name.as_str() != "transfer-encoding" && name.as_str() != "connection" {
            response_headers.insert(name.clone(), value.clone());
        }
    }

    let body_stream = upstream_resp.bytes_stream();
    let body = Body::from_stream(body_stream);

    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = response_headers;
    response.into_response()
}

/// Select a search provider by name or pick the first active one with category='search'
fn select_search_provider(db: &Database, name: Option<&str>) -> Option<crate::models::Provider> {
    if let Some(name) = name {
        if let Ok(Some(p)) = ProviderDao::get_by_name(db, name) {
            if p.status.is_active() {
                return Some(p);
            }
        }
        return None;
    }
    // Auto-select: first active search provider
    if let Ok(providers) = ProviderDao::list_by_category(db, "search") {
        return providers.into_iter().find(|p| p.status.is_active());
    }
    None
}

/// Inject the provider's API key into the request based on auth_method
fn apply_provider_auth(
    mut builder: reqwest::RequestBuilder,
    provider: &crate::models::Provider,
) -> reqwest::RequestBuilder {
    let key = &provider.api_key;
    let method = &provider.auth_method;

    if method.starts_with("header:") {
        let header_name = &method["header:".len()..];
        builder = builder.header(header_name, key.as_str());
    } else if method.starts_with("query:") {
        let param_name = &method["query:".len()..];
        builder = builder.query(&[(param_name, key.as_str())]);
    } else if method.starts_with("body:") {
        // body injection not supported in transparent mode — fall back to bearer
        builder = builder.bearer_auth(key);
    } else {
        // Default: bearer
        builder = builder.bearer_auth(key);
    }
    builder
}
