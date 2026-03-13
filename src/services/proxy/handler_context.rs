use axum::http::HeaderMap;
use tokio::time::Instant;
use chrono::{DateTime, Utc};
use crate::models::{Provider, AppType};
use crate::database::dao::ProviderDao;
use super::error::ProxyError;
use super::server::ProxyState;
use super::utils::extract_session_id;
use super::model_fallback::get_model_fallback_chain;

/// Request context that carries all information needed throughout the request lifecycle
pub struct RequestContext {
    pub start_time: Instant,
    pub request_time: DateTime<Utc>,
    /// Ordered list of providers to try (first = highest priority)
    pub providers: Vec<Provider>,
    /// The model requested by the client (from request body)
    pub request_model: Option<String>,
    /// Session ID extracted from headers or body
    pub session_id: Option<String>,
    /// Request path (e.g. /v1/messages)
    pub request_path: String,
    /// HTTP method
    pub request_method: String,
    /// Model fallback chain (original model + degraded alternatives)
    pub models_to_try: Vec<Option<String>>,
}

impl RequestContext {
    /// Build a RequestContext from the incoming request state
    pub fn new(
        state: &ProxyState,
        headers: &HeaderMap,
        body_bytes: &bytes::Bytes,
        request_path: String,
        request_method: String,
    ) -> Result<Self, ProxyError> {
        let start_time = Instant::now();
        let request_time = Utc::now();

        // Detect client type and log features
        let client_type = detect_client_type(headers);
        log_client_features(headers, &client_type);

        // Parse model and session_id from body
        let (requested_model, session_id) = if let Ok(body_json) = serde_json::from_slice::<serde_json::Value>(body_bytes) {
            let model = body_json.get("model")
                .and_then(|m| m.as_str())
                .map(String::from);
            let session = extract_session_id(headers, &body_json);
            (model, session)
        } else {
            (None, None)
        };

        if let Some(ref sid) = session_id {
            tracing::info!("[Proxy] Session ID: {}", sid);
        }

        // Load active providers from DB (supports hot-switching)
        let mut providers = ProviderDao::get_all_providers(&state.db)
            .map_err(|e| ProxyError::RequestError(format!("Failed to load providers: {}", e)))?
            .into_iter()
            .filter(|p| p.is_active)
            .collect::<Vec<_>>();

        if providers.is_empty() {
            tracing::warn!("[Proxy] No active providers found");
            return Err(ProxyError::NoProvider);
        }

        // Sort/partition providers based on requested model
        if let Some(ref model) = requested_model {
            let (mut matching, mut non_matching): (Vec<_>, Vec<_>) = providers
                .into_iter()
                .partition(|p| p.supports_model(model));

            matching.sort_by_key(|p| (p.priority, p.id));
            non_matching.sort_by_key(|p| (p.priority, p.id));
            matching.extend(non_matching);
            providers = matching;

            if !providers.is_empty() {
                tracing::info!("[Proxy] Model-based routing: model='{}' -> trying {} provider(s)", model, providers.len());
            }
        } else {
            providers.sort_by_key(|p| (p.priority, p.id));
        }

        if providers.is_empty() {
            return Err(ProxyError::NoProvider);
        }

        // Build model fallback chain
        let mut models_to_try = vec![requested_model.clone()];
        if let Some(ref original_model) = requested_model {
            let fallback_chain = get_model_fallback_chain(&state.config, original_model, &providers);
            if !fallback_chain.is_empty() {
                tracing::info!("[ModelFallback] Enabled for model '{}', fallback chain: {:?}", original_model, fallback_chain);
                models_to_try.extend(fallback_chain.into_iter().map(Some));
            }
        }

        Ok(RequestContext {
            start_time,
            request_time,
            providers,
            request_model: requested_model,
            session_id,
            request_path,
            request_method,
            models_to_try,
        })
    }

    /// Build modified body bytes for a fallback model
    pub fn build_fallback_body(&self, body_bytes: &bytes::Bytes, model: &str) -> bytes::Bytes {
        if let Ok(mut body_json) = serde_json::from_slice::<serde_json::Value>(body_bytes) {
            if let Some(obj) = body_json.as_object_mut() {
                obj.insert("model".to_string(), serde_json::Value::String(model.to_string()));
                if let Ok(vec) = serde_json::to_vec(&body_json) {
                    return bytes::Bytes::from(vec);
                }
            }
        }
        body_bytes.clone()
    }

    /// Re-sort providers for a specific model (for fallback iteration)
    pub fn providers_for_model(&self, model: &Option<String>) -> Vec<&Provider> {
        if let Some(ref model) = model {
            let (mut matching, mut non_matching): (Vec<_>, Vec<_>) = self
                .providers
                .iter()
                .partition(|p| p.supports_model(model));
            matching.sort_by_key(|p| (p.priority, p.id));
            non_matching.sort_by_key(|p| (p.priority, p.id));
            matching.extend(non_matching);
            matching
        } else {
            let mut sorted: Vec<_> = self.providers.iter().collect();
            sorted.sort_by_key(|p| (p.priority, p.id));
            sorted
        }
    }
}

/// Detect client type from inbound request headers
fn detect_client_type(headers: &HeaderMap) -> AppType {
    if let Some(ua) = headers.get("user-agent").and_then(|v| v.to_str().ok()) {
        let ua_lower = ua.to_lowercase();
        if ua_lower.contains("codex") {
            return AppType::Codex;
        } else if ua_lower.contains("gemini") {
            return AppType::GeminiCli;
        } else if ua_lower.contains("opencode") {
            return AppType::OpenCode;
        } else if ua_lower.contains("openclaw") {
            return AppType::OpenClaw;
        } else if ua_lower.contains("claude") {
            return AppType::ClaudeCode;
        }
    }

    if let Some(x_app) = headers.get("x-app").and_then(|v| v.to_str().ok()) {
        if x_app == "cli" {
            return AppType::ClaudeCode;
        }
    }

    if headers.contains_key("anthropic-version") {
        return AppType::ClaudeCode;
    }

    AppType::ClaudeCode
}

/// Log client feature headers for debugging
fn log_client_features(headers: &HeaderMap, client_type: &AppType) {
    let feature_keys: &[&str] = &[
        "user-agent", "x-app", "anthropic-version",
        "x-stainless-lang", "x-stainless-arch", "x-stainless-os",
        "x-stainless-runtime", "x-stainless-runtime-version",
        "openai-beta", "openai-organization",
        "x-goog-api-client", "x-goog-api-key",
    ];
    let mut features = Vec::new();
    for &key in feature_keys {
        if let Some(val) = headers.get(key).and_then(|v| v.to_str().ok()) {
            features.push(format!("{}={}", key, val));
        }
    }
    tracing::info!(
        "[Proxy] Client detected: {:?} | features: [{}]",
        client_type,
        if features.is_empty() { "<none>".to_string() } else { features.join(", ") }
    );
}
