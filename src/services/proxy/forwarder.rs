use std::sync::Arc;
use axum::http::HeaderMap;
use crate::models::Provider;
use super::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use super::error::ProxyError;
use super::handler_context::RequestContext;
use super::server::ProxyState;
use super::providers::{get_adapter, adapter::AUTH_HEADER_SKIP_SET};
use super::format_converter;

/// Result of a successful forward operation
pub struct ForwardResult {
    pub response: reqwest::Response,
    pub provider: Provider,
}

/// Handles building and sending HTTP requests to upstream providers
pub struct RequestForwarder {
    http_client: reqwest::Client,
}

impl RequestForwarder {
    pub fn new(http_client: reqwest::Client) -> Self {
        Self { http_client }
    }

    /// Forward with failover: try each provider in the context's provider list
    pub async fn forward_with_retry(
        &self,
        ctx: &RequestContext,
        body_bytes: &bytes::Bytes,
        headers: &HeaderMap,
        method: &axum::http::Method,
        uri: &axum::http::Uri,
        state: &ProxyState,
    ) -> Result<ForwardResult, ProxyError> {
        let path = uri.path();
        let query = uri.query();

        let mut last_error = None;

        // Try each model (original + fallbacks)
        for (model_idx, current_model) in ctx.models_to_try.iter().enumerate() {
            if model_idx > 0 {
                if let Some(ref model) = current_model {
                    tracing::info!("[ModelFallback] Trying fallback model: {}", model);
                }
            }

            // Build modified body if using fallback model
            let modified_body = if model_idx > 0 {
                if let Some(ref model) = current_model {
                    ctx.build_fallback_body(body_bytes, model)
                } else {
                    body_bytes.clone()
                }
            } else {
                body_bytes.clone()
            };

            // Get providers sorted for current model
            let current_providers = ctx.providers_for_model(current_model);

            // Try each provider
            let mut all_circuit_broken = true;
            for (idx, provider) in current_providers.iter().enumerate() {
                // Get or create circuit breaker
                let circuit_breaker = self.get_or_create_circuit_breaker(state, provider).await;

                // Check circuit breaker
                if !circuit_breaker.allow_request().await {
                    let cb_state = circuit_breaker.get_state().await;
                    tracing::warn!(
                        "Provider '{}' skipped due to circuit breaker state: {:?}",
                        provider.name, cb_state
                    );
                    continue;
                }
                all_circuit_broken = false;

                tracing::info!("Trying provider: {} (priority: {})", provider.name, provider.priority);

                match self.forward_single(provider, path, query, &modified_body, headers, method, state, ctx).await {
                    Ok(response) => {
                        tracing::info!("Request succeeded with provider: {}", provider.name);
                        circuit_breaker.record_success().await;
                        return Ok(ForwardResult {
                            response,
                            provider: (*provider).clone(),
                        });
                    }
                    Err(e) => {
                        self.handle_forward_error(&e, provider, &circuit_breaker, idx, current_providers.len()).await;
                        last_error = Some(e);
                    }
                }
            }

            // All providers circuit-broken: reset and retry first
            if all_circuit_broken && !current_providers.is_empty() {
                tracing::warn!("[Proxy] All providers circuit-broken, resetting circuit breakers and retrying");
                self.reset_circuit_breakers(state, &current_providers).await;

                let provider = current_providers[0];
                let circuit_breaker = self.get_or_create_circuit_breaker(state, provider).await;

                tracing::info!("Retrying provider after CB reset: {} (priority: {})", provider.name, provider.priority);

                match self.forward_single(provider, path, query, &modified_body, headers, method, state, ctx).await {
                    Ok(response) => {
                        circuit_breaker.record_success().await;
                        return Ok(ForwardResult {
                            response,
                            provider: (*provider).clone(),
                        });
                    }
                    Err(e) => {
                        circuit_breaker.record_failure().await;
                        last_error = Some(e);
                    }
                }
            }

            if model_idx < ctx.models_to_try.len() - 1 {
                tracing::info!("[ModelFallback] All providers failed for current model, trying next fallback model");
            }
        }

        Err(last_error.unwrap_or(ProxyError::NoProvider))
    }

    // --- private helpers ---

    /// Forward a single request to one provider
    async fn forward_single(
        &self,
        provider: &Provider,
        path: &str,
        query: Option<&str>,
        body_bytes: &bytes::Bytes,
        headers: &HeaderMap,
        method: &axum::http::Method,
        state: &ProxyState,
        ctx: &RequestContext,
    ) -> Result<reqwest::Response, ProxyError> {
        let adapter = get_adapter(provider);
        let base_url = adapter.extract_base_url(provider)?;

        // Parse JSON once, apply model mapping + format conversion, serialize once
        let (final_body, final_model, target_path) = if let Ok(body_json) = serde_json::from_slice::<serde_json::Value>(body_bytes) {
            // 1. Apply model mapping (JSON → JSON, no serialize)
            let (mapped_body, original, mapped) = super::model_mapper::apply_model_mapping(body_json, provider);
            if let (Some(orig), Some(map)) = (&original, &mapped) {
                tracing::info!("[Proxy] Model mapping: {} -> {}", orig, map);
            }
            let final_model = mapped_body.get("model")
                .and_then(|m| m.as_str())
                .map(String::from);

            // 2. Apply format conversion if needed (JSON → JSON, no serialize)
            let needs_conv = provider.needs_format_conversion()
                && ctx.client_format != provider.effective_api_format();
            let (result_body, target_path) = if needs_conv {
                let provider_format = provider.effective_api_format();
                let (converted, new_path) = format_converter::convert_request(
                    ctx.client_format, provider_format, &mapped_body, path,
                );
                tracing::info!(
                    "[FormatConverter] Request converted: {} -> {} | path: {} -> {}",
                    ctx.client_format, provider_format, path, new_path
                );
                (converted, new_path)
            } else {
                (mapped_body, path.to_string())
            };

            // 3. Serialize once at the end
            let body_vec = serde_json::to_vec(&result_body)
                .map_err(|e| ProxyError::RequestError(format!("Failed to serialize body: {}", e)))?;
            (bytes::Bytes::from(body_vec), final_model, target_path)
        } else {
            (body_bytes.clone(), None, path.to_string())
        };

        let target_url = adapter.build_url(&base_url, &target_path, query);

        tracing::info!(
            "[Proxy] >>> Request to Provider: {} | URL: {} | Model: {}",
            provider.name, target_url,
            final_model.as_deref().unwrap_or("<none>")
        );

        let mut request_builder = self.http_client.request(
            reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::POST),
            &target_url,
        );

        // Copy headers, skipping auth, host, adapter-managed, and headers overridden by global/provider headers
        let auth_info = adapter.extract_auth(provider);
        let auth_header_name = auth_info.as_ref().map(|a| a.header_name().to_string());
        let global_hdrs = &state.config.proxy.headers;
        let custom_hdrs = provider.custom_headers();
        // Build lowercase key set for case-insensitive comparison (global + provider)
        let override_keys: std::collections::HashSet<String> = global_hdrs.keys()
            .chain(custom_hdrs.keys())
            .map(|k| k.to_lowercase())
            .collect();
        let managed = adapter.managed_headers();

        for (key, value) in headers.iter() {
            if key == "host" || key == "accept-encoding"
                || AUTH_HEADER_SKIP_SET.iter().any(|&h| key == h)
                || auth_header_name.as_deref().map_or(false, |n| key.as_str() == n)
                || override_keys.contains(key.as_str())
                || managed.iter().any(|&h| key == h)
            {
                continue;
            }
            request_builder = request_builder.header(key, value);
        }

        request_builder = request_builder.header("accept-encoding", "identity");

        // Apply global headers from config.toml [proxy.headers]
        for (key, value) in global_hdrs {
            request_builder = request_builder.header(key.as_str(), value.as_str());
        }

        // Apply provider custom headers (overrides global headers with same name)
        for (key, value) in &custom_hdrs {
            request_builder = request_builder.header(key.as_str(), value.as_str());
        }

        // Add provider-specific headers
        request_builder = adapter.add_provider_headers(request_builder, headers, provider);

        // Inject auth
        if let Some(auth) = auth_info {
            request_builder = adapter.add_auth_headers(request_builder, &auth);
        }

        tracing::info!("[Proxy] >>> {} {} | Body: {} bytes", method, target_url, final_body.len());
        tracing::debug!("[Proxy] >>> Request body:\n{}", String::from_utf8_lossy(&final_body));

        let response = request_builder
            .body(final_body.clone())
            .send()
            .await
            .map_err(|e| {
                tracing::error!("[Proxy] Request failed - Provider: {} | URL: {} | Error: {:?}",
                    provider.name, target_url, e);
                self.log_request_error(&e, &final_body, provider);
                ProxyError::UpstreamError(format!("{:?}", e))
            })?;

        let status = response.status();
        tracing::info!("[Proxy] <<< Response: {} from provider '{}'", status, provider.name);

        if !status.is_success() {
            tracing::warn!("[Proxy] Non-success status: {} from provider: {}", status, provider.name);

            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_else(|_| "Failed to read error body".to_string());

            tracing::warn!("[Proxy] Error response body: {}", error_body);

            return Err(ProxyError::UpstreamError(format!(
                "HTTP {} from provider '{}': {}",
                status, provider.name, error_body
            )));
        }

        Ok(response)
    }

    /// Get or create a circuit breaker for a provider
    async fn get_or_create_circuit_breaker(&self, state: &ProxyState, provider: &Provider) -> Arc<CircuitBreaker> {
        let mut breakers = state.circuit_breakers.write().await;
        breakers.entry(provider.id)
            .or_insert_with(|| {
                Arc::new(CircuitBreaker::new_with_name(
                    CircuitBreakerConfig::default(),
                    provider.name.clone(),
                ))
            })
            .clone()
    }

    /// Handle a forward error: log and update circuit breaker
    async fn handle_forward_error(
        &self,
        error: &ProxyError,
        provider: &Provider,
        circuit_breaker: &CircuitBreaker,
        attempt_idx: usize,
        total_providers: usize,
    ) {
        let error_str = format!("{:?}", error);

        let status_code = if let Some(start) = error_str.find("HTTP ") {
            error_str[start..].split_whitespace().nth(1).unwrap_or("unknown")
        } else {
            "N/A"
        };

        // IncompleteMessage is normal for streaming
        if error_str.contains("IncompleteMessage") {
            tracing::debug!(
                "Provider '{}' completed with IncompleteMessage (normal for streaming)",
                provider.name
            );
            return;
        }

        tracing::warn!(
            "Provider '{}' failed (attempt {}/{}): HTTP {} | Error: {:?}",
            provider.name, attempt_idx + 1, total_providers, status_code, error
        );

        // Don't trigger CB for auth/config errors
        let is_auth_or_config_error = error_str.contains("HTTP 401")
            || error_str.contains("HTTP 403")
            || error_str.contains("HTTP 404")
            || error_str.contains("HTTP 400");

        if !is_auth_or_config_error {
            circuit_breaker.record_failure().await;
            tracing::info!("Provider '{}' CB failure recorded (HTTP {})", provider.name, status_code);
        } else {
            tracing::debug!(
                "Provider '{}' returned auth/config error (HTTP {}), not triggering circuit breaker",
                provider.name, status_code
            );
        }
    }

    /// Reset circuit breakers for all given providers
    async fn reset_circuit_breakers(&self, state: &ProxyState, providers: &[&Provider]) {
        let breakers = state.circuit_breakers.read().await;
        for provider in providers {
            if let Some(cb) = breakers.get(&provider.id) {
                cb.reset().await;
            }
        }
    }

    /// Log detailed request error info
    fn log_request_error(&self, e: &reqwest::Error, body: &bytes::Bytes, provider: &Provider) {
        if e.is_timeout() {
            tracing::error!("[Proxy] Error type: Timeout");
        } else if e.is_connect() {
            tracing::error!("[Proxy] Error type: Connection failed");
            if let Some(url) = e.url() {
                tracing::error!("[Proxy] Failed to connect to: {}", url);
            }
        } else if e.is_request() {
            tracing::error!("[Proxy] Error type: Request error");
        } else if e.is_body() {
            tracing::error!("[Proxy] Error type: Body error");
        } else if e.is_decode() {
            tracing::error!("[Proxy] Error type: Decode error");
        }

        tracing::debug!("[Proxy] Request body: {}",
            String::from_utf8_lossy(body));
        tracing::debug!("[Proxy] Provider details: base_url={}, provider_type={:?}",
            provider.base_url, provider.provider_type);
    }
}
