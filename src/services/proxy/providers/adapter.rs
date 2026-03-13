use axum::http::HeaderMap;
use crate::models::Provider;
use crate::services::proxy::error::ProxyError;
use super::auth::AuthInfo;

/// The set of known auth header names that should be stripped from forwarded requests
pub const AUTH_HEADER_SKIP_SET: &[&str] = &["x-api-key", "authorization", "x-goog-api-key"];

/// Trait defining how a provider-specific adapter builds URLs, injects auth, and transforms requests
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

    /// Build the target URL from base_url, path, and optional query string
    fn build_url(&self, base_url: &str, path: &str, query: Option<&str>) -> String {
        match query {
            Some(q) => format!("{}{}?{}", base_url, path, q),
            None => format!("{}{}", base_url, path),
        }
    }

    /// Add auth headers to the request builder
    fn add_auth_headers(
        &self,
        mut request: reqwest::RequestBuilder,
        auth: &AuthInfo,
    ) -> reqwest::RequestBuilder {
        request = request.header(auth.header_name(), auth.header_value());
        request
    }

    /// Add provider-specific headers beyond auth
    /// Default impl does nothing; adapters override for special headers
    fn add_provider_headers(
        &self,
        request: reqwest::RequestBuilder,
        _headers: &HeaderMap,
        _provider: &Provider,
    ) -> reqwest::RequestBuilder {
        request
    }

    /// Headers that this adapter manages exclusively via add_provider_headers().
    /// The generic header-forwarding loop will skip these to avoid duplicates.
    fn managed_headers(&self) -> &'static [&'static str] {
        &[]
    }
}
