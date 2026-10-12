// Admin API auth middleware — JWT-based

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;
use super::auth::{verify_token, Claims};

/// JWT secret stored in request extensions (injected via layer)
#[derive(Clone)]
pub struct JwtSecret(pub String);

/// Legacy admin token (kept for backward compat)
#[derive(Clone)]
pub struct AdminToken(pub String);

/// JWT auth middleware
/// Extracts Bearer token from Authorization header, verifies JWT,
/// and injects Claims into request extensions.
pub async fn jwt_auth_middleware(
    request: Request,
    next: Next,
) -> Response {
    // Get JWT secret from extensions
    let secret = match request.extensions().get::<JwtSecret>() {
        Some(s) => s.0.clone(),
        None => {
            // No secret configured — reject
            return unauthorized("JWT secret not configured");
        }
    };

    // Extract Bearer token
    let auth_header = request.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let token = match auth_header.strip_prefix("Bearer ") {
        Some(t) => t,
        None => {
            return unauthorized("Missing or invalid Authorization header");
        }
    };

    // Verify token
    match verify_token(token, &secret) {
        Ok(claims) => {
            let mut request = request;
            request.extensions_mut().insert(claims);
            next.run(request).await
        }
        Err(e) => {
            unauthorized(&format!("Invalid token: {}", e))
        }
    }
}

/// Legacy static token auth middleware (backward compat)
pub async fn auth_middleware(
    request: Request,
    next: Next,
) -> Response {
    let expected_token = request.extensions()
        .get::<AdminToken>()
        .map(|t| t.0.clone());

    let expected = match expected_token {
        Some(t) => t,
        None => return next.run(request).await,
    };

    let auth_header = request.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);

    if token == expected {
        next.run(request).await
    } else {
        unauthorized("Unauthorized")
    }
}

fn unauthorized(msg: &str) -> Response {
    let body = json!({
        "code": 401,
        "message": msg,
        "timestamp": chrono::Utc::now().timestamp_millis()
    });
    (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response()
}

/// Helper extractor: get claims from request extensions
/// Used in handlers to access the authenticated user's claims.
pub fn get_claims_from_extensions(extensions: &axum::http::Extensions) -> Option<&Claims> {
    extensions.get::<Claims>()
}
