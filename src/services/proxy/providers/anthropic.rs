use axum::http::HeaderMap;
use crate::models::Provider;
use super::adapter::ProviderAdapter;

/// Anthropic API adapter
pub struct AnthropicAdapter;

impl ProviderAdapter for AnthropicAdapter {
    fn add_provider_headers(
        &self,
        mut request: reqwest::RequestBuilder,
        headers: &HeaderMap,
        _provider: &Provider,
    ) -> reqwest::RequestBuilder {
        // Forward anthropic-version header if present
        if let Some(version) = headers.get("anthropic-version").and_then(|v| v.to_str().ok()) {
            request = request.header("anthropic-version", version);
        }

        // Handle anthropic-beta header:
        // Ensure claude-code-20250219 is included if not already present
        if let Some(beta) = headers.get("anthropic-beta").and_then(|v| v.to_str().ok()) {
            let required_beta = "claude-code-20250219";
            if beta.contains(required_beta) {
                // Already has it, forward as-is
                request = request.header("anthropic-beta", beta);
            } else {
                let new_beta = format!("{},{}", beta, required_beta);
                request = request.header("anthropic-beta", new_beta);
            }
        }

        request
    }

    fn managed_headers(&self) -> &'static [&'static str] {
        &["anthropic-version", "anthropic-beta"]
    }
}
