use std::sync::Arc;
use std::collections::HashSet;
use axum::http::HeaderMap;
use crate::models::{Provider, BillingType};
use super::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use super::channel_selector;
use super::auto_ban;
use super::error::ProxyError;
use super::handler_context::RequestContext;
use super::request_logger::{RequestLogger, RequestLogBuilder};
use super::server::ProxyState;
use super::providers::{get_adapter, adapter::AUTH_HEADER_SKIP_SET};
use super::relay_mode::RelayMode;
use super::format_converter;
use super::format_header_cache;
use super::param_override::{ParamOverrideEngine, OverrideContext};

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

    /// Forward with failover: priority-layered retry with weighted random selection
    ///
    /// Algorithm (aligned with new-api):
    /// - Outer loop: model fallback chain (original + degraded models)
    /// - Inner loop: retry with priority degradation (retry_index → lower priority layer)
    /// - Within each priority layer: weighted random selection
    /// - Auto-ban: disable provider on fatal errors, auto-recover on success
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
        let retry_config = &state.config.retry;
        let max_retries = retry_config.max_retries;

        let mut last_error = None;

        // Try each model (original + fallbacks)
        for (model_idx, current_model) in ctx.models_to_try.iter().enumerate() {
            if model_idx > 0 {
                if let Some(ref model) = current_model {
                    tracing::info!(trace_id = %ctx.trace_id, "[ModelFallback] Trying fallback model: {}", model);
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

            // Get providers for current model
            let current_providers: Vec<&Provider> = ctx.providers_for_model(current_model);

            if current_providers.is_empty() {
                continue;
            }

            // Collect owned providers for channel_selector (it needs &[Provider])
            let provider_refs: Vec<Provider> = current_providers.iter().map(|p| (*p).clone()).collect();
            let mut excluded: HashSet<i64> = HashSet::new();

            // Retry loop with priority degradation
            for retry in 0..=max_retries {
                let selected = channel_selector::select_provider(&provider_refs, retry, &excluded);
                let provider = match selected {
                    Some(p) => p,
                    None => {
                        tracing::info!(
                            trace_id = %ctx.trace_id,
                            "[Retry] No more providers available at retry={}", retry
                        );
                        break;
                    }
                };

                // Circuit breaker check (skip for pinned provider)
                let circuit_breaker = self.get_or_create_circuit_breaker(state, provider).await;
                if !ctx.pinned && !circuit_breaker.allow_request().await {
                    tracing::warn!(
                        trace_id = %ctx.trace_id,
                        "[Retry] Provider '{}' skipped: circuit breaker open", provider.name
                    );
                    excluded.insert(provider.id);
                    continue;
                }

                // Provider-level rate limiting
                if !state.provider_limiter.check_rpm(provider.id, provider.rpm_limit).await {
                    tracing::warn!(
                        trace_id = %ctx.trace_id,
                        "[RateLimit] Provider '{}' RPM limit reached ({})", provider.name, provider.rpm_limit
                    );
                    excluded.insert(provider.id);
                    continue;
                }
                if !state.provider_limiter.check_tpm(provider.id, provider.tpm_limit).await {
                    tracing::warn!(
                        trace_id = %ctx.trace_id,
                        "[RateLimit] Provider '{}' TPM limit reached ({})", provider.name, provider.tpm_limit
                    );
                    excluded.insert(provider.id);
                    continue;
                }
                if !state.provider_limiter.check_concurrency(provider.id, provider.concurrent_limit).await {
                    tracing::warn!(
                        trace_id = %ctx.trace_id,
                        "[RateLimit] Provider '{}' concurrency limit reached ({})", provider.name, provider.concurrent_limit
                    );
                    excluded.insert(provider.id);
                    continue;
                }

                // Acquire concurrency slot before sending request (-1 = unlimited, skip)
                let has_conc_limit = provider.concurrent_limit > 0;
                if has_conc_limit {
                    state.provider_limiter.acquire_concurrency(provider.id).await;
                }

                if retry > 0 {
                    tracing::info!(
                        trace_id = %ctx.trace_id,
                        "[Retry] Attempt {} → provider '{}' (priority: {})",
                        retry, provider.name, provider.priority
                    );
                } else {
                    tracing::info!(
                        trace_id = %ctx.trace_id,
                        "Trying provider: {} (priority: {})", provider.name, provider.priority
                    );
                }

                match self.forward_single(provider, path, query, &modified_body, headers, method, state, ctx).await {
                    Ok(response) => {
                        tracing::info!(trace_id = %ctx.trace_id, "Request succeeded with provider: {}", provider.name);

                        // Release concurrency slot (request handed off to response_processor)
                        if has_conc_limit {
                            state.provider_limiter.release_concurrency(provider.id).await;
                        }

                        // Check if CB was open/half-open before recording success
                        let was_tripped = circuit_breaker.get_state().await != super::circuit_breaker::CircuitState::Closed;
                        circuit_breaker.record_success().await;

                        // Distributed state: reset failure counter, notify recovery if tripped
                        if was_tripped {
                            state.distributed.notify_cb_recovered(provider.id).await;
                        } else {
                            state.distributed.reset_failures(provider.id).await;
                        }

                        // Auto-recover: if provider was auto-banned, re-enable it
                        if retry_config.auto_enable_on_success {
                            // Check if remark indicates auto-ban (best-effort)
                            if let Some(ref remark) = provider.remark {
                                if remark.starts_with("[auto-ban]") {
                                    auto_ban::enable_provider(&state.db, provider.id);
                                    state.distributed.publish_provider_enabled(provider.id).await;
                                }
                            }
                        }

                        return Ok(ForwardResult {
                            response,
                            provider: provider.clone(),
                        });
                    }
                    Err(e) => {
                        excluded.insert(provider.id);

                        // Release concurrency slot on failure
                        if has_conc_limit {
                            state.provider_limiter.release_concurrency(provider.id).await;
                        }

                        // Record circuit breaker failure (skip for auth/config errors)
                        self.handle_forward_error(&e, provider, &circuit_breaker, retry as usize, (max_retries + 1) as usize).await;

                        // Distributed circuit breaker: track failure across instances
                        state.distributed.check_and_trip(provider.id).await;

                        // Auto-ban check
                        if auto_ban::should_disable_provider(&e, provider, retry_config) {
                            auto_ban::disable_provider(&state.db, provider.id, &format!("{:?}", e), retry_config.auto_ban_duration_secs);
                            state.distributed.publish_provider_banned(provider.id).await;
                        }

                        // Record error to request_logs (per-retry, with provider_id)
                        if state.config.proxy.error_log_enabled && e.should_record_error_log() {
                            Self::record_retry_error_log(state, ctx, provider, &e, retry);
                        }

                        // Should we retry?
                        if !auto_ban::should_retry(&e, retry_config) {
                            tracing::info!(
                                trace_id = %ctx.trace_id,
                                "[Retry] Skipping retry: error not retryable (provider '{}')",
                                provider.name
                            );
                            last_error = Some(e);
                            // Break inner retry loop, but continue model fallback
                            break;
                        }

                        last_error = Some(e);
                    }
                }
            }

            if model_idx < ctx.models_to_try.len() - 1 {
                tracing::info!(trace_id = %ctx.trace_id, "[ModelFallback] All providers failed for current model, trying next fallback model");
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
        use crate::database::dao::ProviderEndpointDao;

        let adapter = get_adapter(provider);
        let base_url = adapter.extract_base_url(provider)?;

        // Query endpoint configuration for this provider and path
        let endpoint_type = Self::infer_endpoint_type(path);
        let preferred_format = ctx.client_format.to_string().to_lowercase();
        let endpoint = ProviderEndpointDao::get_best_endpoint(&state.db, provider.id, &endpoint_type, &preferred_format)
            .ok()
            .flatten();

        // Determine provider's API format from endpoint config
        // If no endpoint configured, trust the client's format (passthrough)
        let provider_format = endpoint
            .as_ref()
            .and_then(|ep| ep.api_format.parse::<crate::models::ApiFormat>().ok())
            .unwrap_or(ctx.client_format);

        // Parse JSON once, apply model mapping + format conversion, serialize once
        let (final_body, final_model, target_path) = if let Ok(body_json) = serde_json::from_slice::<serde_json::Value>(body_bytes) {
            // 1. Apply model mapping (JSON → JSON, no serialize)
            let (mapped_body, original, mapped) = super::model_mapper::apply_model_mapping(body_json, provider);
            if let (Some(orig), Some(map)) = (&original, &mapped) {
                tracing::info!(trace_id = %ctx.trace_id, "[Proxy] Model mapping: {} -> {}", orig, map);
            }

            // 1b. Apply upstream_model mapping from provider_models table
            let mapped_body = if let Some(upstream_model) = ctx.model_mappings.get(&provider.id) {
                let mut body = mapped_body;
                if let Some(obj) = body.as_object_mut() {
                    let orig_model = obj.get("model").and_then(|m| m.as_str()).unwrap_or("").to_string();
                    obj.insert("model".to_string(), serde_json::Value::String(upstream_model.clone()));
                    tracing::info!(trace_id = %ctx.trace_id, "[ModelRouter] upstream_model mapping: '{}' -> '{}'", orig_model, upstream_model);
                }
                body
            } else {
                mapped_body
            };

            let final_model = mapped_body.get("model")
                .and_then(|m| m.as_str())
                .map(String::from);

            // 2. Apply ParamOverride before format conversion
            let mut overridden_body = mapped_body;
            let override_ctx = OverrideContext {
                user_id: None, // TODO: extract from request context when user auth is added
                model: final_model.as_deref(),
                provider_name: &provider.name,
            };
            ParamOverrideEngine::apply(
                &mut overridden_body,
                &provider.name, // Use provider name instead of provider_type
                &provider.name,
                &state.db,
                Some(&override_ctx),
            );

            // 3. Apply format conversion if needed
            //    Passthrough: never convert, forward body and use endpoint path as-is
            let needs_conv = ctx.client_format != provider_format
                && provider_format != crate::models::ApiFormat::Passthrough;
            let (result_body, target_path) = if needs_conv {
                // Try the registry first, fall back to legacy converter
                let conversion_result = state.format_registry
                    .get(ctx.client_format, provider_format)
                    .and_then(|converter| converter.convert_request(&overridden_body, path).ok());

                let (converted, new_path) = match conversion_result {
                    Some(result) => result,
                    None => {
                        // Fallback to legacy format_converter
                        format_converter::convert_request(
                            ctx.client_format, provider_format, &overridden_body, path,
                        )
                    }
                };
                tracing::info!(
                    trace_id = %ctx.trace_id,
                    "[FormatConverter] Request converted: {} -> {} | path: {} -> {}",
                    ctx.client_format, provider_format, path, new_path
                );
                (converted, new_path)
            } else {
                // No conversion needed — use endpoint's configured path (not client's request path)
                let ep_path = endpoint.as_ref()
                    .map(|ep| ep.endpoint_path.clone())
                    .unwrap_or_else(|| path.to_string());
                (overridden_body, ep_path)
            };

            // 4. Inject stream_options for OpenAI chat completions streaming
            //    so the upstream returns usage in the final chunk (MRouter consumes it internally,
            //    the usage-only chunk is filtered out before forwarding to downstream clients)
            //    Only for standard chat completions format (has "messages" field), NOT Responses API
            let is_chat_completions = provider_format == crate::models::ApiFormat::OpenAI
                && result_body.get("messages").is_some();
            let result_body = if is_chat_completions
                && result_body.get("stream").and_then(|v| v.as_bool()) == Some(true)
                && result_body.get("stream_options").is_none()
            {
                let mut body = result_body;
                body.as_object_mut().map(|obj| {
                    obj.insert("stream_options".to_string(),
                        serde_json::json!({"include_usage": true}));
                });
                body
            } else {
                result_body
            };

            // 4. Serialize once at the end
            let body_vec = serde_json::to_vec(&result_body)
                .map_err(|e| ProxyError::RequestError(format!("Failed to serialize body: {}", e)))?;
            (bytes::Bytes::from(body_vec), final_model, target_path)
        } else {
            (body_bytes.clone(), None, path.to_string())
        };

        let relay_mode = RelayMode::from_path(&target_path);
        let target_url = adapter.build_request_url(&base_url, &relay_mode, &target_path, query, provider, final_model.as_deref());

        tracing::info!(
            trace_id = %ctx.trace_id,
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
        let custom_hdrs = provider.custom_headers_map();
        // Build lowercase key set for case-insensitive comparison (global + provider)
        let override_keys: std::collections::HashSet<String> = global_hdrs.keys()
            .chain(custom_hdrs.keys())
            .map(|k| k.to_lowercase())
            .collect();
        // Format headers managed by api_format (data-driven, replaces adapter.managed_headers())
        let api_format_str = provider_format.to_string().to_lowercase();
        let format_managed = format_header_cache::managed_headers_for(&api_format_str);

        for (key, value) in headers.iter() {
            if key == "host" || key == "accept-encoding" || key == "content-length" || key == "x-provider-id"
                || AUTH_HEADER_SKIP_SET.iter().any(|&h| key == h)
                || auth_header_name.as_deref().map_or(false, |n| key.as_str() == n)
                || override_keys.contains(key.as_str())
                || format_managed.iter().any(|h| key.as_str() == h)
            {
                continue;
            }
            request_builder = request_builder.header(key, value);
        }

        request_builder = request_builder.header("accept-encoding", "identity");

        // Apply format headers (data-driven, replaces adapter.add_provider_headers())
        request_builder = format_header_cache::apply(request_builder, &api_format_str, headers);

        // Apply global headers from config.toml [proxy.headers]
        for (key, value) in global_hdrs {
            request_builder = request_builder.header(key.as_str(), value.as_str());
        }

        // Apply provider custom headers (overrides global headers with same name)
        for (key, value) in &custom_hdrs {
            request_builder = request_builder.header(key.as_str(), value.as_str());
        }

        // Inject auth
        if let Some(auth) = auth_info {
            request_builder = adapter.add_auth_headers(request_builder, &auth);
        }

        tracing::info!(trace_id = %ctx.trace_id, "[Proxy] >>> {} {} | Body: {} bytes", method, target_url, final_body.len());
        tracing::info!(trace_id = %ctx.trace_id, "[Proxy] >>> Request body:\n{}", String::from_utf8_lossy(&final_body));

        // Build request to inspect headers before sending
        let request_obj = request_builder
            .body(final_body.clone())
            .build()
            .map_err(|e| ProxyError::RequestError(format!("Failed to build request: {}", e)))?;

        // Log outgoing headers
        tracing::info!(trace_id = %ctx.trace_id, "[Proxy] >>> Outgoing headers:");
        for (name, value) in request_obj.headers().iter() {
            tracing::info!(trace_id = %ctx.trace_id, "[Proxy] >>>   {}: {}", name, value.to_str().unwrap_or("<binary>"));
        }

        let response = self.http_client.execute(request_obj)
            .await
            .map_err(|e| {
                tracing::error!(trace_id = %ctx.trace_id, "[Proxy] Request failed - Provider: {} | URL: {} | Error: {:?}",
                    provider.name, target_url, e);
                self.log_request_error(&e, &final_body, provider);
                ProxyError::UpstreamError(format!("{:?}", e))
            })?;

        let status = response.status();
        tracing::info!(trace_id = %ctx.trace_id, "[Proxy] <<< Response: {} from provider '{}'", status, provider.name);

        if !status.is_success() {
            tracing::warn!(trace_id = %ctx.trace_id, "[Proxy] Non-success status: {} from provider: {}", status, provider.name);

            let error_body = response.bytes().await
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_else(|_| "Failed to read error body".to_string());

            tracing::warn!(trace_id = %ctx.trace_id, "[Proxy] Error response body: {}", error_body);

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
                let cb_config = CircuitBreakerConfig::from(&state.config.circuit_breaker);
                Arc::new(CircuitBreaker::new_with_name(
                    cb_config,
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
    #[allow(dead_code)]
    async fn reset_circuit_breakers(&self, state: &ProxyState, providers: &[&Provider]) {
        let breakers = state.circuit_breakers.read().await;
        for provider in providers {
            if let Some(cb) = breakers.get(&provider.id) {
                cb.reset().await;
            }
        }
    }

    /// Infer endpoint type from request path
    fn infer_endpoint_type(path: &str) -> String {
        if path.contains("/chat/completions") || path.contains("/messages") || path.contains("/responses") {
            "chat".to_string()
        } else if path.contains("/completions") && !path.contains("/chat") {
            "completion".to_string()
        } else if path.contains("/embeddings") {
            "embeddings".to_string()
        } else if path.contains("/images/generations") {
            "images".to_string()
        } else if path.contains("/rerank") {
            "rerank".to_string()
        } else if path.contains("/models") {
            "models".to_string()
        } else {
            "chat".to_string() // Default to chat
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

        tracing::info!("[Proxy] Request body: {}",
            String::from_utf8_lossy(body));
        tracing::info!("[Proxy] Provider details: base_url={}, name={}",
            provider.base_url, provider.name);
    }

    /// Record a per-retry error to request_logs with the specific provider that failed
    fn record_retry_error_log(
        state: &ProxyState,
        ctx: &RequestContext,
        provider: &Provider,
        error: &ProxyError,
        retry_index: u32,
    ) {
        let duration_ms = ctx.start_time.elapsed().as_millis() as i64;
        let status_code = error.status_code();
        let error_message = format!("{:?}", error);

        let mut log_builder = RequestLogBuilder::new(provider.id, ctx.request_time)
            .response_time(chrono::Utc::now().timestamp_millis())
            .duration_ms(duration_ms)
            .status_code(status_code)
            .error_message(error_message)
            .request_path(ctx.request_path.clone())
            .request_method(ctx.request_method.clone())
            .provider_name(provider.name.clone())
            .request_id(ctx.trace_id.clone())
            .retry_count(retry_index as i32)
            .billing_type(BillingType::Request);

        if let Some(ref model) = ctx.request_model {
            log_builder = log_builder.model(model.clone());
        }
        if let Some(ref sid) = ctx.session_id {
            log_builder = log_builder.session_id(sid.clone());
        }
        if let Some(key_id) = ctx.api_key_id {
            let key_name = ctx.api_key_name.clone().unwrap_or_default();
            log_builder = log_builder.api_key_info(key_id, key_name);
        }
        if let Some(uid) = ctx.user_id {
            log_builder = log_builder.user_id(uid);
        }
        if let Some(ref ip) = ctx.client_ip {
            log_builder = log_builder.client_ip(ip.clone());
        }

        let log = log_builder.build();

        if let Err(e) = RequestLogger::new(&state.db).log_request(&log) {
            tracing::warn!(
                trace_id = %ctx.trace_id,
                "Failed to record retry error log: {}", e
            );
        } else {
            tracing::info!(
                trace_id = %ctx.trace_id,
                "[Proxy] Recorded error request: provider='{}', retry={}, status={}, duration={}ms",
                provider.name, retry_index, status_code, duration_ms,
            );
        }
    }
}
