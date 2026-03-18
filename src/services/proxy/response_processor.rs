use axum::{
    body::Body,
    response::Response,
};
use chrono::Utc;
use tokio::time::{Duration, Instant};
use crate::models::{Provider, ApiFormat, TokenUsage};
use super::cost::CostCalculator;
use super::error::ProxyError;
use super::format_converter;
use super::handler_context::RequestContext;
use super::request_logger::{RequestLogger, RequestLogBuilder};
use super::server::ProxyState;
use super::utils::{extract_token_usage_with_type, extract_token_usage_from_sse_with_type,
                   extract_token_usage_with_format, extract_token_usage_from_sse_with_format};

/// Process a successful upstream response (auto-detects streaming vs non-streaming)
pub async fn process_response(
    response: reqwest::Response,
    ctx: &RequestContext,
    provider: &Provider,
    state: &ProxyState,
) -> Result<Response, ProxyError> {
    let status = response.status();
    let response_headers = response.headers().clone();

    // Determine if format conversion is needed (api_format explicitly set + differs from client)
    let needs_conversion = provider.needs_format_conversion()
        && ctx.client_format != provider.effective_api_format();
    let client_format = ctx.client_format;
    let provider_format = provider.effective_api_format();

    if needs_conversion {
        tracing::info!(
            "[FormatConverter] Response conversion: {} -> {} for provider '{}'",
            provider_format, client_format, provider.name
        );
    }

    let is_streaming = response_headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("text/event-stream"))
        .unwrap_or(false);

    if is_streaming {
        handle_streaming(response, status, response_headers, ctx, provider, state,
                         needs_conversion, client_format, provider_format)
    } else if !provider.enable_stats && !needs_conversion {
        handle_non_streaming_passthrough(response, status, response_headers)
    } else {
        handle_non_streaming_buffered(response, status, response_headers, ctx, provider, state,
                                      needs_conversion, client_format, provider_format).await
    }
}

/// Handle streaming response with async_stream-based token collection
fn handle_streaming(
    response: reqwest::Response,
    status: reqwest::StatusCode,
    response_headers: reqwest::header::HeaderMap,
    ctx: &RequestContext,
    provider: &Provider,
    state: &ProxyState,
    needs_conversion: bool,
    client_format: ApiFormat,
    provider_format: ApiFormat,
) -> Result<Response, ProxyError> {
    tracing::info!("[Proxy] Detected streaming response, forwarding with token extraction");

    let mut response_builder = Response::builder()
        .status(status.as_u16());

    for (key, value) in response_headers.iter() {
        response_builder = response_builder.header(key.as_str(), value);
    }

    // Clone values needed for the async stream
    let provider_id = provider.id;
    let provider_type = provider.provider_type;
    let enable_stats = provider.enable_stats;
    let provider_name = provider.name.clone();
    let provider_pricing = provider.pricing();
    let final_model = ctx.request_model.clone();
    let session_id = ctx.session_id.clone();
    let request_time = ctx.request_time;
    let request_path = ctx.request_path.clone();
    let request_method = ctx.request_method.clone();
    let request_start = ctx.start_time;
    let db_clone = state.db.clone();
    let streaming_timeout = state.config.proxy.streaming_timeout.clone();

    // Clone conversion flags for async block
    let needs_conversion = needs_conversion;
    let client_format = client_format;
    let provider_format = provider_format;

    let byte_stream = response.bytes_stream();

    // Use async_stream for cleaner streaming with token collection
    let stream = async_stream::stream! {
        use futures::StreamExt;

        let first_byte_timeout = Duration::from_secs(streaming_timeout.first_byte_secs);
        let idle_timeout = Duration::from_secs(streaming_timeout.idle_secs);
        let total_timeout = Duration::from_secs(streaming_timeout.total_secs);

        let stream_start = Instant::now();
        let mut first_chunk_received = false;
        let mut last_chunk_time = Instant::now();
        let mut ttft_ms: Option<u64> = None;

        // SSE event buffer for token extraction and debug logging
        let mut sse_buffer = String::new();
        let mut collected_events: Vec<serde_json::Value> = Vec::new();
        // Raw stream content for debug logging (collected regardless of enable_stats)
        let mut raw_stream_log = String::new();

        tokio::pin!(byte_stream);

        loop {
            // Determine the appropriate timeout
            let timeout_dur = if !first_chunk_received {
                first_byte_timeout
            } else {
                idle_timeout
            };

            // Also check total timeout
            if stream_start.elapsed() > total_timeout {
                tracing::warn!("[Proxy] Total streaming timeout exceeded ({}s)", total_timeout.as_secs());
                yield Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Total timeout exceeded"));
                break;
            }

            let chunk_result = tokio::time::timeout(timeout_dur, byte_stream.next()).await;

            match chunk_result {
                Ok(Some(Ok(bytes))) => {
                    if bytes.is_empty() {
                        continue;
                    }

                    // Record TTFT
                    if !first_chunk_received {
                        first_chunk_received = true;
                        let ttft = stream_start.elapsed().as_millis() as u64;
                        ttft_ms = Some(ttft);
                        tracing::info!("[Proxy] TTFT: {}ms", ttft);
                    }
                    last_chunk_time = Instant::now();

                    // Collect raw stream content for debug logging
                    let chunk_str = String::from_utf8_lossy(&bytes);
                    raw_stream_log.push_str(&chunk_str);

                    // Parse SSE events for token collection (if stats enabled)
                    if enable_stats {
                        sse_buffer.push_str(&chunk_str);
                        parse_sse_events_from_buffer(&mut sse_buffer, &mut collected_events);
                    }

                    // Apply SSE format conversion if needed
                    if needs_conversion {
                        let converted = convert_sse_chunk(&chunk_str, client_format, provider_format);
                        yield Ok(bytes::Bytes::from(converted));
                    } else {
                        yield Ok(bytes);
                    }
                }
                Ok(Some(Err(e))) => {
                    let err_str = e.to_string();
                    if err_str.contains("IncompleteMessage") {
                        tracing::info!("[Proxy] Stream ended (IncompleteMessage is normal for SSE)");
                        break;
                    } else {
                        tracing::warn!("[Proxy] Stream error: {}", err_str);
                        yield Err(std::io::Error::other(err_str));
                        break;
                    }
                }
                Ok(None) => {
                    // Stream ended normally
                    break;
                }
                Err(_) => {
                    if !first_chunk_received {
                        tracing::warn!("[Proxy] First byte timeout exceeded ({}s)", first_byte_timeout.as_secs());
                        yield Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "First byte timeout exceeded"));
                    } else {
                        tracing::warn!("[Proxy] Idle timeout exceeded ({}s)", idle_timeout.as_secs());
                        yield Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Idle timeout exceeded"));
                    }
                    break;
                }
            }
        }

        let _ = last_chunk_time; // suppress unused warning

        // Stream ended — process collected events for stats
        if enable_stats && !collected_events.is_empty() {
            // Check for error events in the stream
            for event in &collected_events {
                if let Some(event_type) = event.get("type").and_then(|v| v.as_str()) {
                    if event_type == "error" {
                        tracing::error!("[Proxy] Error event in SSE stream from provider '{}': {:?}",
                            provider_name, event.get("error"));
                    }
                }
                if let Some(error) = event.get("error") {
                    tracing::error!("[Proxy] Error in SSE stream from provider '{}': {:?}",
                        provider_name, error);
                }
            }

            // Build raw SSE data from collected events for token extraction
            let raw_data = rebuild_sse_data(&collected_events);

            // When format conversion is active, use provider's api_format to select the correct parser
            // (e.g., provider_type=Custom but api_format=OpenAI → use OpenAI parser)
            let usage_opt = if needs_conversion {
                extract_token_usage_from_sse_with_format(&raw_data, provider_format)
            } else {
                extract_token_usage_from_sse_with_type(&raw_data, &provider_type)
            };
            if let Some(usage) = usage_opt {
                let cost = CostCalculator::calculate_simple(&usage, &provider_pricing);
                let duration_ms = request_start.elapsed().as_millis() as i64;

                record_streaming_stats(
                    &db_clone, provider_id, request_time, duration_ms,
                    &usage, cost, &final_model, ttft_ms,
                    &session_id, &request_path, &request_method,
                );
            } else {
                tracing::warn!("[Proxy] Failed to extract token usage from SSE stream");
            }
        } else if enable_stats {
            tracing::warn!("[Proxy] No SSE events collected for token extraction");
        }

        // Log full streaming response content at debug level
        tracing::debug!("[Proxy] <<< Streaming response ({} bytes):\n{}",
            raw_stream_log.len(), raw_stream_log);
    };

    let body = Body::from_stream(stream);

    let response = response_builder
        .body(body)
        .map_err(|e| ProxyError::ResponseError(e.to_string()))?;

    Ok(response)
}

/// Handle non-streaming response (stats disabled) — pure passthrough
fn handle_non_streaming_passthrough(
    response: reqwest::Response,
    status: reqwest::StatusCode,
    response_headers: reqwest::header::HeaderMap,
) -> Result<Response, ProxyError> {
    tracing::info!("[Proxy] Processing non-streaming response (pass-through, stats disabled)");
    use futures::StreamExt;

    let stream = response
        .bytes_stream()
        .map(|chunk| chunk.map_err(|e| std::io::Error::other(e.to_string())));

    let body = Body::from_stream(stream);

    let mut response_builder = Response::builder()
        .status(status.as_u16());
    for (key, value) in response_headers.iter() {
        response_builder = response_builder.header(key.as_str(), value);
    }

    let response = response_builder
        .body(body)
        .map_err(|e| ProxyError::ResponseError(e.to_string()))?;

    Ok(response)
}

/// Handle non-streaming response (stats enabled or format conversion needed) — buffer and extract tokens
async fn handle_non_streaming_buffered(
    response: reqwest::Response,
    status: reqwest::StatusCode,
    response_headers: reqwest::header::HeaderMap,
    ctx: &RequestContext,
    provider: &Provider,
    state: &ProxyState,
    needs_conversion: bool,
    client_format: ApiFormat,
    provider_format: ApiFormat,
) -> Result<Response, ProxyError> {
    tracing::info!("[Proxy] Processing non-streaming response (buffered, stats enabled)");

    let body_bytes = response.bytes().await
        .map_err(|e| {
            tracing::error!("[Proxy] Failed to read response body: {:?}", e);
            ProxyError::ResponseError(e.to_string())
        })?;

    tracing::info!("[Proxy] Response body: {} bytes", body_bytes.len());
    tracing::debug!("[Proxy] <<< Response body:\n{}", String::from_utf8_lossy(&body_bytes));

    // Parse JSON once and reuse for both token extraction and format conversion
    let parsed_json = serde_json::from_slice::<serde_json::Value>(&body_bytes).ok();

    // Extract token usage from parsed JSON
    let usage_info = if let Some(ref json) = parsed_json {
        tracing::info!("[Proxy] Response body parsed as JSON");

        // Check for error in response
        if let Some(error) = json.get("error") {
            tracing::warn!("[Proxy] API returned error: {:?}", error);

            let error_message = if let Some(msg) = error.get("message").and_then(|v| v.as_str()) {
                msg.to_string()
            } else {
                serde_json::to_string(error).unwrap_or_else(|_| "Unknown error".to_string())
            };

            return Err(ProxyError::UpstreamError(format!(
                "API error from provider '{}': {}",
                provider.name, error_message
            )));
        }

        // When format conversion is active, use provider's api_format to select the correct parser
        if needs_conversion {
            extract_token_usage_with_format(json, provider_format)
        } else {
            extract_token_usage_with_type(json, &provider.provider_type)
        }
    } else {
        tracing::info!("[Proxy] Response body is not JSON");
        None
    };

    // Apply format conversion to response body if needed (reuse parsed JSON)
    let final_body = if needs_conversion {
        if let Some(json) = parsed_json {
            let converted = format_converter::convert_response(client_format, provider_format, &json);
            tracing::info!("[FormatConverter] Non-streaming response converted: {} -> {}", provider_format, client_format);
            bytes::Bytes::from(serde_json::to_vec(&converted).unwrap_or_else(|_| body_bytes.to_vec()))
        } else {
            body_bytes
        }
    } else {
        body_bytes
    };

    let mut response_builder = Response::builder()
        .status(status.as_u16());
    for (key, value) in response_headers.iter() {
        // Update content-length if body was converted
        if needs_conversion && key.as_str() == "content-length" {
            continue;
        }
        // Update content-type for Anthropic client expecting Anthropic response
        if needs_conversion && key.as_str() == "content-type" {
            continue;
        }
        response_builder = response_builder.header(key.as_str(), value);
    }
    if needs_conversion {
        response_builder = response_builder
            .header("content-type", "application/json")
            .header("content-length", final_body.len().to_string());
    }

    let response = response_builder
        .body(Body::from(final_body))
        .map_err(|e| ProxyError::ResponseError(e.to_string()))?;

    // Return response with extracted usage for the handler to record
    // We attach usage via the UsageCarrier pattern — the handler processes it
    if let Some(usage) = usage_info {
        let pricing = provider.pricing();
        let cost = CostCalculator::calculate_simple(&usage, &pricing);
        let duration_ms = ctx.start_time.elapsed().as_millis() as i64;

        record_non_streaming_stats(
            &state.db, ctx, provider, &usage, cost, duration_ms,
            status.as_u16() as i32,
        );
    }

    Ok(response)
}

/// Convert an SSE chunk (may contain multiple events) from provider format to client format
fn convert_sse_chunk(chunk: &str, client_format: ApiFormat, provider_format: ApiFormat) -> String {
    let mut result = String::new();

    for line in chunk.split('\n') {
        if let Some(data) = line.strip_prefix("data: ") {
            let converted = format_converter::convert_sse_event(client_format, provider_format, data);
            // convert_sse_event may return multi-line SSE with event: prefixes already
            if converted.contains("event: ") || converted.contains("data: ") {
                result.push_str(&converted);
            } else if !converted.is_empty() {
                result.push_str("data: ");
                result.push_str(&converted);
                result.push('\n');
            }
        } else {
            // Pass through non-data lines (event:, id:, empty lines)
            // But skip "event:" lines when converting, as convert_sse_event generates its own
            if !line.starts_with("event:") || client_format == provider_format {
                result.push_str(line);
                result.push('\n');
            }
        }
    }

    result
}

/// Parse complete SSE events from a buffer, leaving incomplete data in the buffer
fn parse_sse_events_from_buffer(buffer: &mut String, events: &mut Vec<serde_json::Value>) {
    while let Some(pos) = buffer.find("\n\n") {
        let event_text = buffer[..pos].to_string();
        *buffer = buffer[pos + 2..].to_string();

        if event_text.trim().is_empty() {
            continue;
        }

        for line in event_text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                let data = data.trim();
                if data == "[DONE]" || data.is_empty() {
                    continue;
                }
                if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(data) {
                    events.push(json_value);
                }
            }
        }
    }

    // Also try to parse individual lines that may not have double-newline separators
    // (some providers send events without proper SSE framing in the tail)
    let remaining = buffer.clone();
    for line in remaining.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            let data = data.trim();
            if data == "[DONE]" || data.is_empty() {
                continue;
            }
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(data) {
                events.push(json_value);
            }
        }
    }
    // Don't clear buffer for incomplete lines — they may be completed by next chunk
}

/// Rebuild raw SSE bytes from collected JSON events (for token_parser compatibility)
fn rebuild_sse_data(events: &[serde_json::Value]) -> Vec<u8> {
    let mut data = Vec::new();
    for event in events {
        let line = format!("data: {}\n\n", event);
        data.extend_from_slice(line.as_bytes());
    }
    data
}

/// Record stats for a streaming response
fn record_streaming_stats(
    db: &crate::database::Database,
    provider_id: i64,
    request_time: chrono::DateTime<chrono::Utc>,
    duration_ms: i64,
    usage: &TokenUsage,
    cost: f64,
    final_model: &Option<String>,
    ttft_ms: Option<u64>,
    session_id: &Option<String>,
    request_path: &str,
    request_method: &str,
) {
    let mut log_builder = RequestLogBuilder::new(provider_id, request_time)
        .response_time(Utc::now())
        .duration_ms(duration_ms)
        .status_code(200)
        .usage(usage)
        .cost(cost)
        .request_path(request_path.to_string())
        .request_method(request_method.to_string());

    if let Some(model) = final_model {
        log_builder = log_builder.model(model.clone());
    }
    if let Some(ttft) = ttft_ms {
        log_builder = log_builder.first_token_ms(ttft as i64);
    }
    if let Some(sid) = session_id {
        log_builder = log_builder.session_id(sid.clone());
    }

    let log = log_builder.build();

    if let Err(e) = RequestLogger::new(db).log_request(&log) {
        tracing::warn!("Failed to record streaming request log: {}", e);
    } else {
        tracing::info!(
            "[Proxy] Recorded streaming request: duration={}ms, ttft={:?}ms, tokens={}, cost=${:.6}",
            duration_ms, ttft_ms, usage.total_tokens(), cost
        );
    }

    // Also record aggregate stats
    if let Err(e) = RequestLogger::new(db).log_usage_stats(provider_id, 1, usage, cost) {
        tracing::warn!("Failed to record streaming usage stats: {}", e);
    }
}

/// Record stats for a non-streaming response (writes to database)
fn record_non_streaming_stats(
    db: &crate::database::Database,
    ctx: &RequestContext,
    provider: &Provider,
    usage: &TokenUsage,
    cost: f64,
    duration_ms: i64,
    status_code: i32,
) {
    // Record detailed request log
    let mut log_builder = RequestLogBuilder::new(provider.id, ctx.request_time)
        .response_time(Utc::now())
        .duration_ms(duration_ms)
        .status_code(status_code)
        .model(ctx.request_model.clone().unwrap_or_else(|| "unknown".to_string()))
        .usage(usage)
        .cost(cost)
        .request_path(ctx.request_path.clone())
        .request_method(ctx.request_method.clone());

    if let Some(ref sid) = ctx.session_id {
        log_builder = log_builder.session_id(sid.clone());
    }

    let log = log_builder.build();

    if let Err(e) = RequestLogger::new(db).log_request(&log) {
        tracing::warn!("Failed to log request: {}", e);
    } else {
        tracing::info!(
            "[Proxy] Recorded non-streaming request: duration={}ms, tokens={}, cost=${:.6}",
            duration_ms, usage.total_tokens(), cost
        );
    }

    // Record aggregate stats
    if let Err(e) = RequestLogger::new(db).log_usage_stats(provider.id, 1, usage, cost) {
        tracing::warn!("Failed to record usage stats: {}", e);
    }
}
