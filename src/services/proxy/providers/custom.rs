use crate::models::Provider;
use crate::services::proxy::relay_mode::RelayMode;
use super::adapter::{ProviderAdapter, append_query};

/// Generic custom provider adapter — true passthrough for user-configured endpoints.
/// The user is responsible for setting the correct base_url; we just append the client path.
pub struct CustomAdapter;

impl ProviderAdapter for CustomAdapter {
    fn build_request_url(
        &self,
        base_url: &str,
        _relay_mode: &RelayMode,
        client_path: &str,
        query: Option<&str>,
        _provider: &Provider,
        _model: Option<&str>,
    ) -> String {
        // Custom providers do passthrough — user controls the full URL via base_url + client path
        append_query(format!("{}{}", base_url, client_path), query)
    }
}
