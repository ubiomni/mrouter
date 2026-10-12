//! Admin API handlers for Search Provider management
//!
//! GET  /api/search/providers         — list search-category providers
//! POST /api/search/test              — test a search query via transparent proxy

use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};

use crate::database::dao::ProviderDao;
use super::auth::Claims;
use super::handlers::{ok, err};
use super::permission;
use super::server::AdminState;

/// GET /api/search/providers — list providers with category=search
pub async fn list_search_providers(
    State(state): State<AdminState>,
    request: axum::http::Request<axum::body::Body>,
) -> impl IntoResponse {
    let claims = match request.extensions().get::<Claims>() {
        Some(c) => c.clone(),
        None => return err(401, "Unauthorized").into_response(),
    };
    if !permission::is_org_admin(&claims) {
        return err(403, "Only org_admin can view search providers").into_response();
    }

    match ProviderDao::list_by_category(&state.db, "search") {
        Ok(providers) => ok(json!(providers)).into_response(),
        Err(e) => err(500, &format!("Failed to list search providers: {}", e)).into_response(),
    }
}

/// POST /api/search/test — test search against a provider (transparent proxy test)
pub async fn test_search(
    State(state): State<AdminState>,
    request: axum::http::Request<axum::body::Body>,
) -> impl IntoResponse {
    let (parts, body) = request.into_parts();
    let claims = match parts.extensions.get::<Claims>() {
        Some(c) => c.clone(),
        None => return err(401, "Unauthorized").into_response(),
    };
    if !permission::is_org_admin(&claims) {
        return err(403, "Only org_admin can test search").into_response();
    }

    let json: Value = match axum::body::to_bytes(body, 512 * 1024).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        Err(_) => return err(400, "Invalid request body").into_response(),
    };

    let provider_id = match json.get("provider_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => return err(400, "provider_id is required").into_response(),
    };

    let provider = match ProviderDao::get_by_id(&state.db, provider_id) {
        Ok(Some(p)) => p,
        Ok(None) => return err(404, "Provider not found").into_response(),
        Err(e) => return err(500, &format!("DB error: {}", e)).into_response(),
    };

    // Build a test request: POST to provider's base_url with the body
    let client = reqwest::Client::new();
    let base = provider.base_url.trim_end_matches('/');
    let search_path = json.get("path").and_then(|v| v.as_str()).unwrap_or("");

    // Smart path handling: if base_url already ends with the requested path, don't append
    let url = if search_path.is_empty() {
        base.to_string()
    } else if base.ends_with(search_path.trim_start_matches('/')) {
        // base_url already contains the full path, use it directly
        base.to_string()
    } else {
        format!("{}{}", base, search_path)
    };

    let auth_method = &provider.auth_method;
    let api_key = &provider.api_key;

    // Build upstream body: use provided "body" field or construct from "query"
    let upstream_body = if let Some(body_val) = json.get("body") {
        body_val.to_string()
    } else {
        let query = json.get("query").and_then(|v| v.as_str()).unwrap_or("test");
        json!({"query": query}).to_string()
    };

    // For query-based APIs (e.g. SearchAPI): convert JSON body to query params + GET
    let mut req_builder = if auth_method.starts_with("query:") {
        let param_name = &auth_method["query:".len()..];
        // Parse body JSON into query params
        let mut params: Vec<(String, String)> = Vec::new();
        if let Ok(obj) = serde_json::from_str::<Value>(&upstream_body) {
            if let Some(map) = obj.as_object() {
                for (k, v) in map {
                    match v {
                        Value::String(s) => params.push((k.clone(), s.clone())),
                        Value::Number(n) => params.push((k.clone(), n.to_string())),
                        Value::Bool(b) => params.push((k.clone(), b.to_string())),
                        _ => {}
                    }
                }
            }
        }
        params.push((param_name.to_string(), api_key.trim().to_string()));
        client.get(&url).query(&params)
    } else {
        let mut rb = client.post(&url)
            .header("Content-Type", "application/json")
            .body(upstream_body);

        // Apply auth
        if auth_method == "bearer" || auth_method.is_empty() {
            rb = rb.header("Authorization", format!("Bearer {}", api_key.trim()));
        } else if let Some(header_name) = auth_method.strip_prefix("header:") {
            rb = rb.header(header_name, api_key.trim());
        }
        rb
    };

    match req_builder.timeout(std::time::Duration::from_secs(15)).send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body_text = resp.text().await.unwrap_or_default();
            let parsed: Value = serde_json::from_str(&body_text).unwrap_or(json!({"raw": body_text}));
            ok(json!({
                "status": status,
                "response": parsed,
                "provider": provider.name,
            })).into_response()
        }
        Err(e) => {
            err(502, &format!("Upstream error: {}", e)).into_response()
        }
    }
}
