//! Proxy 错误类型 — 根据请求协议返回对应格式的 JSON 错误响应
//!
//! - /v1/messages (Anthropic): {"type":"error","error":{"type":"...","message":"..."}}
//! - /v1/chat/completions, /v1/responses (OpenAI): {"error":{"message":"...","type":"...","code":"..."}}

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Client API format (determines error response shape)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorFormat {
    OpenAI,
    Anthropic,
}

/// Proxy 错误类型
#[derive(Debug)]
pub enum ProxyError {
    NoProvider,
    ProviderNotFound(i64),
    ModelNotFound(String),
    RequestError(String),
    UpstreamError(String),
    ResponseError(String),
    AuthError { status: u16, message: String },
}

impl ProxyError {
    /// HTTP status code for this error (used for error logging)
    pub fn status_code(&self) -> i32 {
        let (status, _, _) = self.decompose_ref();
        status.as_u16() as i32
    }

    /// Convert to response with the appropriate error format based on request path.
    pub fn into_response_with_format(self, format: ErrorFormat) -> Response {
        let (status, error_type, message) = self.decompose();

        let body = match format {
            ErrorFormat::Anthropic => json!({
                "type": "error",
                "error": {
                    "type": error_type,
                    "message": message,
                }
            }),
            ErrorFormat::OpenAI => json!({
                "error": {
                    "message": message,
                    "type": error_type,
                    "code": error_type,
                }
            }),
        };

        (status, Json(body)).into_response()
    }

    /// Detect format from request path
    pub fn format_from_path(path: &str) -> ErrorFormat {
        if path.contains("/v1/messages") {
            ErrorFormat::Anthropic
        } else {
            ErrorFormat::OpenAI
        }
    }

    fn decompose(self) -> (StatusCode, &'static str, String) {
        match self {
            ProxyError::NoProvider => (
                StatusCode::SERVICE_UNAVAILABLE,
                "overloaded_error",
                "No provider available".to_string(),
            ),
            ProxyError::ProviderNotFound(id) => (
                StatusCode::NOT_FOUND,
                "not_found_error",
                format!("Provider with id {} not found", id),
            ),
            ProxyError::ModelNotFound(model) => (
                StatusCode::NOT_FOUND,
                "not_found_error",
                format!("Model '{}' is not available. No provider supports this model.", model),
            ),
            ProxyError::RequestError(msg) => (StatusCode::BAD_REQUEST, "invalid_request_error", msg),
            ProxyError::UpstreamError(msg) => (StatusCode::BAD_GATEWAY, "api_error", msg),
            ProxyError::ResponseError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "api_error", msg),
            ProxyError::AuthError { status, message } => {
                let code = StatusCode::from_u16(status).unwrap_or(StatusCode::FORBIDDEN);
                let err_type = match status {
                    401 => "authentication_error",
                    429 => "rate_limit_error",
                    _ => "permission_error",
                };
                (code, err_type, message)
            }
        }
    }

    /// Non-consuming version — returns only the status code
    fn decompose_ref(&self) -> (StatusCode, &'static str, &str) {
        match self {
            ProxyError::NoProvider => (StatusCode::SERVICE_UNAVAILABLE, "overloaded_error", "No provider available"),
            ProxyError::ProviderNotFound(_) => (StatusCode::NOT_FOUND, "not_found_error", ""),
            ProxyError::ModelNotFound(_) => (StatusCode::NOT_FOUND, "not_found_error", ""),
            ProxyError::RequestError(_) => (StatusCode::BAD_REQUEST, "invalid_request_error", ""),
            ProxyError::UpstreamError(_) => (StatusCode::BAD_GATEWAY, "api_error", ""),
            ProxyError::ResponseError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "api_error", ""),
            ProxyError::AuthError { status, .. } => {
                let code = StatusCode::from_u16(*status).unwrap_or(StatusCode::FORBIDDEN);
                let err_type = match status {
                    401 => "authentication_error",
                    429 => "rate_limit_error",
                    _ => "permission_error",
                };
                (code, err_type, "")
            }
        }
    }

    /// Whether this error should be recorded to request_logs.
    /// Aligned with new-api: default true, only exclude client-side request
    /// errors (bad body / missing params) that never reached upstream.
    pub fn should_record_error_log(&self) -> bool {
        !matches!(self, ProxyError::RequestError(_))
    }
}

/// Fallback IntoResponse — defaults to OpenAI format when path is unknown.
/// Prefer using `into_response_with_format` when the request path is available.
impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        self.into_response_with_format(ErrorFormat::OpenAI)
    }
}
