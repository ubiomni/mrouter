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
use super::sse_collector::{SseUsageCollector, take_sse_block, strip_sse_field};
use super::utils::{extract_token_usage_with_type, extract_token_usage_from_sse_with_type,
                   extract_token_usage_with_format, extract_token_usage_from_sse_with_format};

/// Build an Anthropic SSE error event for incomplete generation.
/// Upstream started thinking but produced no text output before dropping the stream.
fn build_incomplete_generation_error_sse(message: &str) -> String {
    let escaped = message.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "event: error\ndata: {{\"type\":\"error\",\"error\":{{\"type\":\"incomplete_generation\",\"message\":\"{escaped}\"}}}}\n\n"
    )
}

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
            trace_id = %ctx.trace_id,
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
        handle_non_streaming_passthrough(response, status, response_headers, &ctx.trace_id)
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
    tracing::info!(trace_id = %ctx.trace_id, "[Proxy] Detected streaming response, forwarding with token extraction");

    let mut response_builder = Response::builder()
        .status(status.as_u16());

    for (key, value) in response_headers.iter() {
        response_builder = response_builder.header(key.as_str(), value);
    }
    // Inject trace_id into streaming response headers
    response_builder = response_builder.header("x-trace-id", &ctx.trace_id);

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
    let needs_sse_processing = enable_stats;
    let streaming_timeout = state.config.proxy.streaming_timeout.clone();
    let incomplete_generation_message = state.config.proxy.incomplete_generation_message.clone();
    let trace_id = ctx.trace_id.clone();
    let trace_id_header = trace_id.clone();

    // Clone conversion flags for async block
    let needs_conversion = needs_conversion;
    let client_format = client_format;
    let provider_format = provider_format;

    let byte_stream = response.bytes_stream();

    // SseUsageCollector: shared between stream (push) and background task (finish)
    let collector = SseUsageCollector::new(std::time::Instant::now());
    let collector_stream = collector.clone();
    let collector_bg = collector.clone();
    // Oneshot channel to signal stream completion (data lives in collector)
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();

    let stream = async_stream::stream! {
        use futures::StreamExt;

        let first_byte_timeout = if streaming_timeout.first_byte_secs < 0 {
            None
        } else {
            Some(Duration::from_secs(streaming_timeout.first_byte_secs as u64))
        };
        let idle_timeout = if streaming_timeout.idle_secs < 0 {
            None
        } else {
            Some(Duration::from_secs(streaming_timeout.idle_secs as u64))
        };
        let total_timeout = if streaming_timeout.total_secs < 0 {
            None
        } else {
            Some(Duration::from_secs(streaming_timeout.total_secs as u64))
        };

        let stream_start = Instant::now();
        let mut first_chunk_received = false;
        let mut last_chunk_time = Instant::now();

        // SSE event buffer for token extraction and debug logging
        let mut sse_buffer = String::new();
        // Raw stream content for logging
        let mut raw_stream_log = String::new();
        // UTF-8 remainder bytes from previous chunk (for multi-byte char boundary handling)
        let mut utf8_remainder: Vec<u8> = Vec::new();
        // Track whether any actual content was generated (text_delta / choices with content)
        let mut has_output_content = false;
        // Track whether thinking_delta was received (incomplete generation fingerprint)
        let mut has_thinking_delta = false;

        tokio::pin!(byte_stream);

        loop {
            // Determine the appropriate timeout
            let timeout_dur = if !first_chunk_received {
                first_byte_timeout
            } else {
                idle_timeout
            };

            // Check total timeout (skip if disabled)
            if let Some(total) = total_timeout {
                if stream_start.elapsed() > total {
                    tracing::warn!(trace_id = %trace_id, "[Proxy] Total streaming timeout exceeded ({}s)", total.as_secs());
                    yield Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Total timeout exceeded"));
                    break;
                }
            }

            // If timeout is disabled (None), wait indefinitely
            let chunk_result = match timeout_dur {
                Some(dur) => tokio::time::timeout(dur, byte_stream.next()).await,
                None => Ok(byte_stream.next().await),
            };

            match chunk_result {
                Ok(Some(Ok(bytes))) => {
                    if bytes.is_empty() {
                        continue;
                    }

                    // Record first chunk
                    if !first_chunk_received {
                        first_chunk_received = true;
                        tracing::info!(trace_id = %trace_id, "[Proxy] TTFT: {}ms", stream_start.elapsed().as_millis());
                    }
                    last_chunk_time = Instant::now();

                    // Safe UTF-8 decoding across chunk boundaries (cc-switch pattern)
                    let mut chunk_str = String::new();
                    append_utf8_safe(&mut chunk_str, &mut utf8_remainder, &bytes);
                    raw_stream_log.push_str(&chunk_str);

                    // Parse SSE events for token collection (stats or quota)
                    if needs_sse_processing {
                        sse_buffer.push_str(&chunk_str);
                        while let Some(block) = take_sse_block(&mut sse_buffer) {
                            let mut forward_block = String::new();
                            let mut has_content = false;

                            for line in block.lines() {
                                if let Some(data) = strip_sse_field(line, "data") {
                                    let data = data.trim();
                                    if data == "[DONE]" || data.is_empty() {
                                        forward_block.push_str(line);
                                        forward_block.push('\n');
                                        has_content = true;
                                        continue;
                                    }
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                        // Collect for usage parsing (convert to Anthropic if needed)
                                        let event = if needs_conversion && provider_format != ApiFormat::Anthropic {
                                            convert_event_to_anthropic(&json, provider_format)
                                        } else {
                                            json.clone()
                                        };
                                        collector_stream.push(event).await;

                                        // Detect actual output content (Anthropic text_delta or OpenAI choices with content)
                                        // thinking_delta does NOT count — MiniMax sends thinking_delta then drops on incomplete generation
                                        if !has_output_content || !has_thinking_delta {
                                            let event_type = json.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                            let delta_type = json.get("delta")
                                                .and_then(|d| d.get("type"))
                                                .and_then(|t| t.as_str())
                                                .unwrap_or("");
                                            if event_type == "content_block_delta" && delta_type == "thinking_delta" {
                                                has_thinking_delta = true;
                                            }
                                            let is_anthropic_content = event_type == "content_block_delta"
                                                && delta_type == "text_delta";
                                            let is_openai_content = json.get("choices")
                                                .and_then(|c| c.as_array())
                                                .map_or(false, |arr| !arr.is_empty() && arr.iter().any(|c|
                                                    c.get("delta").and_then(|d| d.get("content")).is_some()));
                                            if is_anthropic_content || is_openai_content {
                                                has_output_content = true;
                                            }
                                        }

                                        // Filter usage-only chunks (choices:[] + usage)
                                        let is_usage_only = json.get("usage").is_some()
                                            && json.get("choices")
                                                .and_then(|c| c.as_array())
                                                .map_or(false, |arr| arr.is_empty());
                                        if is_usage_only {
                                            continue; // don't forward to client
                                        }
                                    }
                                    // Forward: convert if needed
                                    if needs_conversion {
                                        let converted = format_converter::convert_sse_event(
                                            client_format, provider_format, data
                                        );
                                        if converted.contains("event: ") || converted.contains("data: ") {
                                            forward_block.push_str(&converted);
                                        } else if !converted.is_empty() {
                                            forward_block.push_str("data: ");
                                            forward_block.push_str(&converted);
                                            forward_block.push('\n');
                                        }
                                    } else {
                                        forward_block.push_str(line);
                                        forward_block.push('\n');
                                    }
                                    has_content = true;
                                } else {
                                    // Non-data lines (event:, id:, etc.)
                                    // Skip original event: lines when converting (converter generates its own)
                                    if needs_conversion && line.starts_with("event:") {
                                        continue;
                                    }
                                    forward_block.push_str(line);
                                    forward_block.push('\n');
                                }
                            }

                            if has_content && !forward_block.is_empty() {
                                forward_block.push('\n'); // SSE block delimiter
                                yield Ok(bytes::Bytes::from(forward_block));
                            }
                        }
                    } else if needs_conversion {
                        // No stats/quota — still need complete blocks for correct conversion
                        sse_buffer.push_str(&chunk_str);
                        while let Some(block) = take_sse_block(&mut sse_buffer) {
                            let mut forward_block = String::new();
                            for line in block.lines() {
                                if let Some(data) = strip_sse_field(line, "data") {
                                    let converted = format_converter::convert_sse_event(
                                        client_format, provider_format, data.trim()
                                    );
                                    if converted.contains("event: ") || converted.contains("data: ") {
                                        forward_block.push_str(&converted);
                                    } else if !converted.is_empty() {
                                        forward_block.push_str("data: ");
                                        forward_block.push_str(&converted);
                                        forward_block.push('\n');
                                    }
                                } else if !line.starts_with("event:") {
                                    forward_block.push_str(line);
                                    forward_block.push('\n');
                                }
                            }
                            if !forward_block.is_empty() {
                                forward_block.push('\n');
                                yield Ok(bytes::Bytes::from(forward_block));
                            }
                        }
                    } else {
                        // No stats, no quota, no conversion — zero-copy forward
                        // Track thinking_delta and text_delta for incomplete generation detection
                        if !has_thinking_delta && chunk_str.contains("thinking_delta") {
                            has_thinking_delta = true;
                        }
                        if !has_output_content && (chunk_str.contains("text_delta")
                            || (chunk_str.contains("\"content\":") && !chunk_str.contains("thinking_delta")))
                        {
                            has_output_content = true;
                        }
                        yield Ok(bytes);
                    }
                }
                Ok(Some(Err(e))) => {
                    let err_str = e.to_string();
                    if err_str.contains("IncompleteMessage") {
                        tracing::info!(trace_id = %trace_id, "[Proxy] Stream ended (IncompleteMessage is normal for SSE)");
                        break;
                    } else {
                        // Content moderation fingerprint:
                        // thinking_delta received + no text_delta output + stream dropped
                        // Upstream started thinking but never produced text output before disconnecting.
                        if has_thinking_delta && !has_output_content && !err_str.contains("timeout") {
                            tracing::warn!(
                                trace_id = %trace_id,
                                "[Proxy] Incomplete generation detected (thinking_delta without text_delta): \
                                 raw_bytes={} raw_content={}",
                                raw_stream_log.len(), raw_stream_log
                            );
                            let synthetic = build_incomplete_generation_error_sse(&incomplete_generation_message);
                            yield Ok(bytes::Bytes::from(synthetic));
                            break;
                        }
                        tracing::warn!(trace_id = %trace_id, "[Proxy] Stream error: {}", err_str);
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
                        let secs = first_byte_timeout.map(|d| d.as_secs()).unwrap_or(0);
                        tracing::warn!(trace_id = %trace_id, "[Proxy] First byte timeout exceeded ({}s)", secs);
                        yield Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "First byte timeout exceeded"));
                    } else {
                        let secs = idle_timeout.map(|d| d.as_secs()).unwrap_or(0);
                        tracing::warn!(trace_id = %trace_id, "[Proxy] Idle timeout exceeded ({}s)", secs);
                        yield Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Idle timeout exceeded"));
                    }
                    break;
                }
            }
        }

        let _ = last_chunk_time; // suppress unused warning

        // Flush any remaining data in sse_buffer (incomplete last block without trailing \n\n)
        if !sse_buffer.is_empty() {
            let remaining = std::mem::take(&mut sse_buffer);
            if needs_sse_processing || needs_conversion {
                // Try to parse remaining as a final block
                for line in remaining.lines() {
                    if let Some(data) = strip_sse_field(line, "data") {
                        let data = data.trim();
                        if data == "[DONE]" || data.is_empty() { continue; }
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            if needs_sse_processing {
                                let event = if needs_conversion && provider_format != ApiFormat::Anthropic {
                                    convert_event_to_anthropic(&json, provider_format)
                                } else {
                                    json
                                };
                                collector_stream.push(event).await;
                            }
                        }
                    }
                }
            }
        }

        // Log full streaming response content
        tracing::info!(trace_id = %trace_id, "[Proxy] <<< Streaming response ({} bytes):\n{}",
            raw_stream_log.len(), raw_stream_log);

        // Signal stream completion to background task
        let _ = done_tx.send(());
    };

    let body = Body::from_stream(stream);

    // Spawn background task to process stats/quota OUTSIDE the stream generator
    // This runs after the stream is consumed, does not block stream EOF
    tokio::spawn(async move {
        // Wait for stream to finish
        let _ = done_rx.await;

        let trace_id = &trace_id_header;

        // Consume collected events from the shared collector
        let (collected_events, ttft_ms) = collector_bg.finish().await;

        // Extract token usage (needed for both stats and quota)
        // When needs_conversion is true, collected_events were converted to Anthropic
        // format during streaming — use AnthropicParser uniformly (cc-switch pattern)
        let usage_opt = if !collected_events.is_empty() {
            let raw_data = rebuild_sse_data(&collected_events);
            if needs_conversion {
                extract_token_usage_from_sse_with_format(&raw_data, ApiFormat::Anthropic)
            } else {
                extract_token_usage_from_sse_with_type(&raw_data, &provider_type)
            }
        } else {
            None
        };

        // Record stats (only when enable_stats is true)
        if enable_stats && !collected_events.is_empty() {
            for event in &collected_events {
                if let Some(event_type) = event.get("type").and_then(|v| v.as_str()) {
                    if event_type == "error" {
                        tracing::error!(trace_id = %trace_id, "[Proxy] Error event in SSE stream from provider '{}': {:?}",
                            provider_name, event.get("error"));
                    }
                }
                if let Some(error) = event.get("error") {
                    tracing::error!(trace_id = %trace_id, "[Proxy] Error in SSE stream from provider '{}': {:?}",
                        provider_name, error);
                }
            }

            if let Some(ref usage) = usage_opt {
                let cost = CostCalculator::calculate_simple(usage, &provider_pricing);
                let duration_ms = request_start.elapsed().as_millis() as i64;

                record_streaming_stats(
                    &db_clone, provider_id, request_time, duration_ms,
                    usage, cost, &final_model, ttft_ms,
                    &session_id, &request_path, &request_method,
                    trace_id,
                );
            } else {
                tracing::warn!(trace_id = %trace_id, "[Proxy] Failed to extract token usage from SSE stream");
            }
        } else if enable_stats {
            tracing::warn!(trace_id = %trace_id, "[Proxy] No SSE events collected for token extraction");
        }
    });

    let response = response_builder
        .body(body)
        .map_err(|e| ProxyError::ResponseError(e.to_string()))?;

    Ok(response)
}

/// Handle non-streaming response (stats disabled) — stream-through with debug logging
fn handle_non_streaming_passthrough(
    response: reqwest::Response,
    status: reqwest::StatusCode,
    response_headers: reqwest::header::HeaderMap,
    trace_id: &str,
) -> Result<Response, ProxyError> {
    tracing::info!(trace_id = %trace_id, "[Proxy] Processing non-streaming response (pass-through, stats disabled)");

    let byte_stream = response.bytes_stream();
    let trace_id = trace_id.to_string();
    let trace_id_header = trace_id.clone();

    let stream = async_stream::stream! {
        use futures::StreamExt;

        let mut raw_log = String::new();
        tokio::pin!(byte_stream);

        while let Some(chunk) = byte_stream.next().await {
            match chunk {
                Ok(bytes) => {
                    raw_log.push_str(&String::from_utf8_lossy(&bytes));
                    yield Ok::<_, std::io::Error>(bytes);
                }
                Err(e) => {
                    yield Err(std::io::Error::other(e.to_string()));
                    break;
                }
            }
        }

        tracing::info!(trace_id = %trace_id, "[Proxy] <<< Response body ({} bytes):\n{}", raw_log.len(), raw_log);
    };

    let body = Body::from_stream(stream);

    let mut response_builder = Response::builder()
        .status(status.as_u16());
    for (key, value) in response_headers.iter() {
        response_builder = response_builder.header(key.as_str(), value);
    }
    response_builder = response_builder.header("x-trace-id", &*trace_id_header);

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
    tracing::info!(trace_id = %ctx.trace_id, "[Proxy] Processing non-streaming response (buffered, stats enabled)");

    let body_bytes = response.bytes().await
        .map_err(|e| {
            tracing::error!(trace_id = %ctx.trace_id, "[Proxy] Failed to read response body: {:?}", e);
            ProxyError::ResponseError(e.to_string())
        })?;

    tracing::info!(trace_id = %ctx.trace_id, "[Proxy] Response body: {} bytes", body_bytes.len());
    tracing::info!(trace_id = %ctx.trace_id, "[Proxy] <<< Response body:\n{}", String::from_utf8_lossy(&body_bytes));

    // Parse JSON once and reuse for both token extraction and format conversion
    let parsed_json = serde_json::from_slice::<serde_json::Value>(&body_bytes).ok();

    // Extract token usage from parsed JSON
    let usage_info = if let Some(ref json) = parsed_json {
        tracing::info!(trace_id = %ctx.trace_id, "[Proxy] Response body parsed as JSON");

        // Check for error in response
        if let Some(error) = json.get("error") {
            tracing::warn!(trace_id = %ctx.trace_id, "[Proxy] API returned error: {:?}", error);

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
        tracing::info!(trace_id = %ctx.trace_id, "[Proxy] Response body is not JSON");
        None
    };

    // Apply format conversion to response body if needed (reuse parsed JSON)
    let final_body = if needs_conversion {
        if let Some(json) = parsed_json {
            let converted = format_converter::convert_response(client_format, provider_format, &json);
            tracing::info!(trace_id = %ctx.trace_id, "[FormatConverter] Non-streaming response converted: {} -> {}", provider_format, client_format);
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
    response_builder = response_builder.header("x-trace-id", &ctx.trace_id);

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

/// Filter out usage-only SSE chunks that were caused by stream_options injection.
/// OpenAI sends a final chunk with "choices":[] and "usage":{...} when stream_options.include_usage=true.
/// This chunk confuses downstream clients (e.g. OpenClaw) and should not be forwarded.
/// Convert a single SSE JSON event to Anthropic format for unified parsing
fn convert_event_to_anthropic(json: &serde_json::Value, provider_format: ApiFormat) -> serde_json::Value {
    let data_str = serde_json::to_string(json).unwrap_or_default();
    let converted = format_converter::convert_sse_event(
        ApiFormat::Anthropic, provider_format, &data_str
    );
    for line in converted.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            let data = data.trim();
            if data != "[DONE]" && !data.is_empty() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    return parsed;
                }
            }
        }
    }
    json.clone() // fallback: conversion failed, keep original
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
    trace_id: &str,
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
        tracing::warn!(trace_id = %trace_id, "Failed to record streaming request log: {}", e);
    } else {
        tracing::info!(
            trace_id = %trace_id,
            "[Proxy] Recorded streaming request: duration={}ms, ttft={:?}ms, tokens={}, cost=${:.6}",
            duration_ms, ttft_ms, usage.total_tokens(), cost
        );
    }

    // Also record aggregate stats
    if let Err(e) = RequestLogger::new(db).log_usage_stats(provider_id, 1, usage, cost) {
        tracing::warn!(trace_id = %trace_id, "Failed to record streaming usage stats: {}", e);
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
        tracing::warn!(trace_id = %ctx.trace_id, "Failed to log request: {}", e);
    } else {
        tracing::info!(
            trace_id = %ctx.trace_id,
            "[Proxy] Recorded non-streaming request: duration={}ms, tokens={}, cost=${:.6}",
            duration_ms, usage.total_tokens(), cost
        );
    }

    // Record aggregate stats
    if let Err(e) = RequestLogger::new(db).log_usage_stats(provider.id, 1, usage, cost) {
        tracing::warn!(trace_id = %ctx.trace_id, "Failed to record usage stats: {}", e);
    }
}

/// Append raw bytes to a UTF-8 buffer, correctly handling multi-byte characters
/// split across chunk boundaries. Ported from cc-switch's sse.rs.
///
/// `remainder` accumulates trailing bytes from the previous chunk that form an
/// incomplete UTF-8 sequence (at most 3 bytes). On each call the remainder is
/// prepended to `new_bytes`, the longest valid UTF-8 prefix is appended to
/// `buffer`, and any trailing incomplete bytes are saved back into `remainder`.
fn append_utf8_safe(buffer: &mut String, remainder: &mut Vec<u8>, new_bytes: &[u8]) {
    let (owned, bytes): (Option<Vec<u8>>, &[u8]) = if remainder.is_empty() {
        (None, new_bytes)
    } else {
        if remainder.len() > 3 {
            // Defensive: remainder should never exceed 3 bytes with valid UTF-8.
            // Flush lossy and start fresh.
            buffer.push_str(&String::from_utf8_lossy(remainder));
            remainder.clear();
            (None, new_bytes)
        } else {
            let mut combined = std::mem::take(remainder);
            combined.extend_from_slice(new_bytes);
            (Some(combined), &[])
        }
    };
    let input = owned.as_deref().unwrap_or(bytes);

    let mut pos = 0;
    loop {
        match std::str::from_utf8(&input[pos..]) {
            Ok(s) => {
                buffer.push_str(s);
                return;
            }
            Err(e) => {
                let valid_up_to = pos + e.valid_up_to();
                // Safety: from_utf8 guarantees [pos..valid_up_to] is valid UTF-8.
                buffer.push_str(std::str::from_utf8(&input[pos..valid_up_to]).unwrap());
                if let Some(invalid_len) = e.error_len() {
                    // Genuinely invalid byte(s) — emit U+FFFD and continue.
                    buffer.push('\u{FFFD}');
                    pos = valid_up_to + invalid_len;
                } else {
                    // Incomplete trailing sequence — stash for next chunk.
                    *remainder = input[valid_up_to..].to_vec();
                    return;
                }
            }
        }
    }
}
