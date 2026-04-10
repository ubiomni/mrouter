//! Proxy 错误类型

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// Proxy 错误类型
#[derive(Debug)]
pub enum ProxyError {
    NoProvider,
    ProviderNotFound(i64),
    RequestError(String),
    UpstreamError(String),
    ResponseError(String),
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ProxyError::NoProvider => (
                StatusCode::SERVICE_UNAVAILABLE,
                "No provider configured".to_string(),
            ),
            ProxyError::ProviderNotFound(id) => (
                StatusCode::NOT_FOUND,
                format!("Provider with id {} not found", id),
            ),
            ProxyError::RequestError(msg) => (StatusCode::BAD_REQUEST, msg),
            ProxyError::UpstreamError(msg) => (StatusCode::BAD_GATEWAY, msg),
            ProxyError::ResponseError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        (status, message).into_response()
    }
}
