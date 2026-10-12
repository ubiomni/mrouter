// Provider Endpoint Model — represents endpoint configuration in endpoint-driven architecture

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEndpoint {
    #[serde(default)]
    pub id: i64,
    pub provider_id: i64,
    pub endpoint_type: String,         // "chat", "completion", "embedding", "image", "models"
    pub api_format: String,             // "openai", "anthropic", "google", etc.
    pub endpoint_path: String,          // e.g., "/v1/chat/completions"
    pub http_method: String,            // "GET", "POST", etc.
    pub required_params: Option<serde_json::Value>, // JSON array of required params
    pub optional_params: Option<serde_json::Value>, // JSON object of defaults
    pub auth_location: String,          // "header", "query", "body"
    pub custom_headers: Option<serde_json::Value>,  // JSON object of custom headers
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ProviderEndpoint {
    pub fn new(
        provider_id: i64,
        endpoint_type: impl Into<String>,
        api_format: impl Into<String>,
        endpoint_path: impl Into<String>,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: 0,
            provider_id,
            endpoint_type: endpoint_type.into(),
            api_format: api_format.into(),
            endpoint_path: endpoint_path.into(),
            http_method: "POST".to_string(),
            required_params: None,
            optional_params: None,
            auth_location: "header".to_string(),
            custom_headers: None,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }
}
