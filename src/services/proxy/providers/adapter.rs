use crate::models::Provider;
use crate::services::proxy::error::ProxyError;
use crate::services::proxy::relay_mode::RelayMode;
use super::auth::AuthInfo;

/// The set of known auth header names that should be stripped from forwarded requests
pub const AUTH_HEADER_SKIP_SET: &[&str] = &["x-api-key", "authorization", "x-goog-api-key"];

/// Trait defining how a provider-specific adapter builds URLs and injects auth.
///
/// Protocol-level headers are now handled by format_header_cache (data-driven).
pub trait ProviderAdapter: Send + Sync {
    /// Extract the base URL from the provider config
    fn extract_base_url(&self, provider: &Provider) -> Result<String, ProxyError> {
        if provider.base_url.is_empty() {
            return Err(ProxyError::RequestError(format!(
                "Provider '{}' has no base URL configured",
                provider.name
            )));
        }
        Ok(provider.base_url.trim_end_matches('/').to_string())
    }

    /// Extract auth info from the provider
    fn extract_auth(&self, provider: &Provider) -> Option<AuthInfo> {
        if provider.api_key.is_empty() {
            return None;
        }
        Some(AuthInfo::from_provider(provider))
    }

    /// Build the full upstream URL for this request.
    ///
    /// `model` is the resolved model name from the request body (after mapping).
    fn build_request_url(
        &self,
        base_url: &str,
        relay_mode: &RelayMode,
        client_path: &str,
        query: Option<&str>,
        provider: &Provider,
        model: Option<&str>,
    ) -> String;

    /// Add auth headers to the request builder
    fn add_auth_headers(
        &self,
        mut request: reqwest::RequestBuilder,
        auth: &AuthInfo,
    ) -> reqwest::RequestBuilder {
        request = request.header(auth.header_name(), auth.header_value());
        request
    }
}

/// Helper: append query string to a URL
pub(crate) fn append_query(url: String, query: Option<&str>) -> String {
    match query {
        Some(q) => format!("{}?{}", url, q),
        None => url,
    }
}
