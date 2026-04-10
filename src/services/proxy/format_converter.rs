use serde_json::{json, Value};
use crate::models::ApiFormat;

/// Convert request body and path from client format to target provider format.
/// Returns (converted_body, target_path).
/// If formats are the same, returns body and path unchanged.
pub fn convert_request(client: ApiFormat, target: ApiFormat, body: &Value, path: &str) -> (Value, String) {
    match (client, target) {
        (ApiFormat::Anthropic, ApiFormat::OpenAI) => anthropic_to_openai_request(body, path),
        (ApiFormat::OpenAI, ApiFormat::Anthropic) => openai_to_anthropic_request(body, path),
        _ => (body.clone(), path.to_string()),
    }
}

/// Convert non-streaming response body from provider format back to client format.
/// If formats are the same, returns body unchanged.
pub fn convert_response(client: ApiFormat, target: ApiFormat, body: &Value) -> Value {
    match (client, target) {
        (ApiFormat::Anthropic, ApiFormat::OpenAI) => openai_to_anthropic_response(body),
        (ApiFormat::OpenAI, ApiFormat::Anthropic) => anthropic_to_openai_response(body),
        _ => body.clone(),
    }
}

/// Convert a single SSE event data line from provider format back to client format.
/// Input/output is the raw string after "data: " prefix.
/// If formats are the same, returns data unchanged.
pub fn convert_sse_event(client: ApiFormat, target: ApiFormat, data: &str) -> String {
    if client == target {
        return data.to_string();
    }
    match (client, target) {
        // Client speaks Anthropic, provider speaks OpenAI → convert OpenAI SSE to Anthropic SSE
        (ApiFormat::Anthropic, ApiFormat::OpenAI) => convert_openai_sse_to_anthropic(data),
        // Client speaks OpenAI, provider speaks Anthropic → convert Anthropic SSE to OpenAI SSE
        (ApiFormat::OpenAI, ApiFormat::Anthropic) => convert_anthropic_sse_to_openai(data),
        _ => data.to_string(),
    }
}

// ─── Anthropic → OpenAI request ───────────────────────────────────────

fn anthropic_to_openai_request(body: &Value, _path: &str) -> (Value, String) {
    let mut result = serde_json::Map::new();

    // Copy model
    if let Some(model) = body.get("model") {
        result.insert("model".to_string(), model.clone());
    }

    // Build messages array
    let mut messages = Vec::new();

    // Convert top-level system to system message
    if let Some(system) = body.get("system") {
        let system_content = extract_system_text(system);
        if !system_content.is_empty() {
            messages.push(json!({"role": "system", "content": system_content}));
        }
    }

    // Convert messages
    if let Some(Value::Array(msgs)) = body.get("messages") {
        for msg in msgs {
            if let Some(converted) = convert_anthropic_msg_to_openai(msg) {
                messages.push(converted);
            }
        }
    }

    result.insert("messages".to_string(), Value::Array(messages));

    // Copy max_tokens
    if let Some(max_tokens) = body.get("max_tokens") {
        result.insert("max_tokens".to_string(), max_tokens.clone());
    }

    // Copy temperature
    if let Some(temp) = body.get("temperature") {
        result.insert("temperature".to_string(), temp.clone());
    }

    // Copy top_p
    if let Some(top_p) = body.get("top_p") {
        result.insert("top_p".to_string(), top_p.clone());
    }

    // Copy stream
    if let Some(stream) = body.get("stream") {
        result.insert("stream".to_string(), stream.clone());
        // When converting to OpenAI format with streaming, inject stream_options
        // so that usage info is included in the final chunk for token stats
        if stream.as_bool() == Some(true) {
            result.insert("stream_options".to_string(), json!({"include_usage": true}));
        }
    }

    // Copy stop_sequences → stop
    if let Some(stop) = body.get("stop_sequences") {
        result.insert("stop".to_string(), stop.clone());
    }

    (Value::Object(result), "/v1/chat/completions".to_string())
}

fn extract_system_text(system: &Value) -> String {
    match system {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            arr.iter()
                .filter_map(|block| {
                    if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                        block.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => String::new(),
    }
}

fn convert_anthropic_msg_to_openai(msg: &Value) -> Option<Value> {
    let role = msg.get("role")?.as_str()?;
    let content = msg.get("content")?;

    let openai_content = match content {
        Value::String(_) => content.clone(),
        Value::Array(blocks) => {
            // If all blocks are text, simplify to a single string
            let texts: Vec<&str> = blocks.iter()
                .filter_map(|b| {
                    if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                        b.get("text").and_then(|t| t.as_str())
                    } else {
                        None
                    }
                })
                .collect();

            if texts.len() == blocks.len() && texts.len() == 1 {
                Value::String(texts[0].to_string())
            } else if texts.len() == blocks.len() {
                Value::String(texts.join("\n"))
            } else {
                // Mixed content (e.g. tool_use, tool_result) — keep as-is for best effort
                content.clone()
            }
        }
        _ => content.clone(),
    };

    Some(json!({"role": role, "content": openai_content}))
}

// ─── OpenAI → Anthropic request ───────────────────────────────────────

fn openai_to_anthropic_request(body: &Value, _path: &str) -> (Value, String) {
    let mut result = serde_json::Map::new();

    // Copy model
    if let Some(model) = body.get("model") {
        result.insert("model".to_string(), model.clone());
    }

    let mut anthropic_messages = Vec::new();

    // Extract system from first message if role=system
    if let Some(Value::Array(msgs)) = body.get("messages") {
        let mut iter = msgs.iter().peekable();

        // Extract leading system messages
        let mut system_parts = Vec::new();
        while let Some(msg) = iter.peek() {
            if msg.get("role").and_then(|r| r.as_str()) == Some("system") {
                if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                    system_parts.push(content.to_string());
                }
                iter.next();
            } else {
                break;
            }
        }
        if !system_parts.is_empty() {
            result.insert("system".to_string(), Value::String(system_parts.join("\n")));
        }

        // Convert remaining messages
        for msg in iter {
            if let Some(converted) = convert_openai_msg_to_anthropic(msg) {
                anthropic_messages.push(converted);
            }
        }
    }

    result.insert("messages".to_string(), Value::Array(anthropic_messages));

    // max_tokens / max_completion_tokens → max_tokens
    if let Some(mt) = body.get("max_tokens").or_else(|| body.get("max_completion_tokens")) {
        result.insert("max_tokens".to_string(), mt.clone());
    } else {
        // Anthropic requires max_tokens
        result.insert("max_tokens".to_string(), json!(4096));
    }

    // Copy temperature
    if let Some(temp) = body.get("temperature") {
        result.insert("temperature".to_string(), temp.clone());
    }

    // Copy top_p
    if let Some(top_p) = body.get("top_p") {
        result.insert("top_p".to_string(), top_p.clone());
    }

    // Copy stream
    if let Some(stream) = body.get("stream") {
        result.insert("stream".to_string(), stream.clone());
    }

    // stop → stop_sequences
    if let Some(stop) = body.get("stop") {
        result.insert("stop_sequences".to_string(), stop.clone());
    }

    (Value::Object(result), "/v1/messages".to_string())
}

fn convert_openai_msg_to_anthropic(msg: &Value) -> Option<Value> {
    let role = msg.get("role")?.as_str()?;
    let content = msg.get("content")?;

    let anthropic_content = match content {
        Value::String(s) => {
            json!([{"type": "text", "text": s}])
        }
        _ => content.clone(),
    };

    Some(json!({"role": role, "content": anthropic_content}))
}

// ─── OpenAI → Anthropic non-streaming response ───────────────────────

fn openai_to_anthropic_response(body: &Value) -> Value {
    let content_text = body
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|msg| msg.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    let stop_reason = body
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("finish_reason"))
        .and_then(|r| r.as_str())
        .map(|r| convert_finish_reason_to_anthropic(r))
        .unwrap_or("end_turn".to_string());

    let mut result = json!({
        "id": body.get("id").cloned().unwrap_or(json!("msg_converted")),
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": content_text}],
        "model": body.get("model").cloned().unwrap_or(json!("unknown")),
        "stop_reason": stop_reason,
    });

    // Convert usage
    if let Some(usage) = body.get("usage") {
        result["usage"] = json!({
            "input_tokens": usage.get("prompt_tokens").cloned().unwrap_or(json!(0)),
            "output_tokens": usage.get("completion_tokens").cloned().unwrap_or(json!(0)),
        });
    }

    result
}

// ─── Anthropic → OpenAI non-streaming response ───────────────────────

fn anthropic_to_openai_response(body: &Value) -> Value {
    let content_text = body
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
        })
        .and_then(|block| block.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    let finish_reason = body
        .get("stop_reason")
        .and_then(|r| r.as_str())
        .map(|r| convert_stop_reason_to_openai(r))
        .unwrap_or("stop".to_string());

    let mut result = json!({
        "id": body.get("id").cloned().unwrap_or(json!("chatcmpl-converted")),
        "object": "chat.completion",
        "model": body.get("model").cloned().unwrap_or(json!("unknown")),
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": content_text,
            },
            "finish_reason": finish_reason,
        }],
    });

    // Convert usage
    if let Some(usage) = body.get("usage") {
        result["usage"] = json!({
            "prompt_tokens": usage.get("input_tokens").cloned().unwrap_or(json!(0)),
            "completion_tokens": usage.get("output_tokens").cloned().unwrap_or(json!(0)),
            "total_tokens": usage.get("input_tokens").and_then(|i| i.as_i64()).unwrap_or(0)
                + usage.get("output_tokens").and_then(|o| o.as_i64()).unwrap_or(0),
        });
    }

    result
}

// ─── SSE conversion: OpenAI → Anthropic ──────────────────────────────

fn convert_openai_sse_to_anthropic(data: &str) -> String {
    let data = data.trim();
    if data == "[DONE]" {
        // OpenAI [DONE] → Anthropic message_stop
        return format_anthropic_sse("message_stop", &json!({}));
    }

    let Ok(chunk) = serde_json::from_str::<Value>(data) else {
        return data.to_string();
    };

    let choice = chunk.get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first());

    let Some(choice) = choice else {
        // No choices (e.g. usage-only chunk) — pass through as ping
        return format_anthropic_sse("ping", &json!({}));
    };

    let empty_obj = json!({});
    let delta = choice.get("delta").unwrap_or(&empty_obj);
    let finish_reason = choice.get("finish_reason");

    // Role-only delta → message_start
    if delta.get("role").is_some() && delta.get("content").is_none() {
        let model = chunk.get("model").and_then(|m| m.as_str()).unwrap_or("unknown");
        let msg_start = json!({
            "type": "message_start",
            "message": {
                "id": chunk.get("id").cloned().unwrap_or(json!("msg_converted")),
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": model,
                "stop_reason": null,
                "usage": {"input_tokens": 0, "output_tokens": 0}
            }
        });
        let block_start = json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}});
        return format!(
            "event: message_start\ndata: {}\n\nevent: content_block_start\ndata: {}\n\n",
            serde_json::to_string(&msg_start).unwrap_or_default(),
            serde_json::to_string(&block_start).unwrap_or_default(),
        );
    }

    // Content delta → content_block_delta
    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
        let block_delta = json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "text_delta", "text": content}
        });
        let mut result = format!(
            "event: content_block_delta\ndata: {}\n\n",
            serde_json::to_string(&block_delta).unwrap_or_default()
        );

        // If finish_reason is present, also emit stop events
        if let Some(fr) = finish_reason.and_then(|f| f.as_str()) {
            result.push_str(&emit_anthropic_stop_events(fr, &chunk));
        }

        return result;
    }

    // finish_reason without content → stop events
    if let Some(fr) = finish_reason.and_then(|f| f.as_str()) {
        return emit_anthropic_stop_events(fr, &chunk);
    }

    // Fallback: pass as ping
    format_anthropic_sse("ping", &json!({}))
}

fn emit_anthropic_stop_events(finish_reason: &str, chunk: &Value) -> String {
    let stop_reason = convert_finish_reason_to_anthropic(finish_reason);
    let block_stop = json!({"type": "content_block_stop", "index": 0});

    let mut usage = json!({"output_tokens": 0});
    if let Some(u) = chunk.get("usage") {
        if let Some(ct) = u.get("completion_tokens") {
            usage["output_tokens"] = ct.clone();
        }
    }

    let msg_delta = json!({
        "type": "message_delta",
        "delta": {"stop_reason": stop_reason},
        "usage": usage
    });
    let msg_stop = json!({"type": "message_stop"});

    format!(
        "event: content_block_stop\ndata: {}\n\nevent: message_delta\ndata: {}\n\nevent: message_stop\ndata: {}\n\n",
        serde_json::to_string(&block_stop).unwrap_or_default(),
        serde_json::to_string(&msg_delta).unwrap_or_default(),
        serde_json::to_string(&msg_stop).unwrap_or_default(),
    )
}

fn format_anthropic_sse(event_type: &str, data: &Value) -> String {
    format!(
        "event: {}\ndata: {}\n\n",
        event_type,
        serde_json::to_string(data).unwrap_or_default()
    )
}

// ─── SSE conversion: Anthropic → OpenAI ──────────────────────────────

fn convert_anthropic_sse_to_openai(data: &str) -> String {
    let data = data.trim();

    // Try to parse — some Anthropic events come as raw "event: X\ndata: {}" blocks
    // but our caller strips the "data: " prefix, so we get the JSON payload.
    let Ok(event) = serde_json::from_str::<Value>(data) else {
        return data.to_string();
    };

    let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match event_type {
        "message_start" => {
            let empty = json!({});
            let msg = event.get("message").unwrap_or(&empty);
            let chunk = json!({
                "id": msg.get("id").cloned().unwrap_or(json!("chatcmpl-converted")),
                "object": "chat.completion.chunk",
                "model": msg.get("model").cloned().unwrap_or(json!("unknown")),
                "choices": [{
                    "index": 0,
                    "delta": {"role": "assistant"},
                    "finish_reason": null
                }]
            });
            format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default())
        }
        "content_block_delta" => {
            let text = event.get("delta")
                .and_then(|d| d.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let chunk = json!({
                "id": "chatcmpl-converted",
                "object": "chat.completion.chunk",
                "choices": [{
                    "index": 0,
                    "delta": {"content": text},
                    "finish_reason": null
                }]
            });
            format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default())
        }
        "message_delta" => {
            let stop_reason = event.get("delta")
                .and_then(|d| d.get("stop_reason"))
                .and_then(|r| r.as_str())
                .unwrap_or("stop");
            let finish_reason = convert_stop_reason_to_openai(stop_reason);

            let mut chunk = json!({
                "id": "chatcmpl-converted",
                "object": "chat.completion.chunk",
                "choices": [{
                    "index": 0,
                    "delta": {},
                    "finish_reason": finish_reason
                }]
            });

            // Forward usage if present
            if let Some(usage) = event.get("usage") {
                chunk["usage"] = json!({
                    "completion_tokens": usage.get("output_tokens").cloned().unwrap_or(json!(0)),
                });
            }

            format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default())
        }
        "message_stop" => {
            "data: [DONE]\n\n".to_string()
        }
        // content_block_start, content_block_stop, ping — skip/ignore
        _ => String::new(),
    }
}

// ─── Helper: stop/finish reason mapping ──────────────────────────────

fn convert_finish_reason_to_anthropic(reason: &str) -> String {
    match reason {
        "stop" => "end_turn".to_string(),
        "length" => "max_tokens".to_string(),
        "content_filter" => "end_turn".to_string(),
        _ => "end_turn".to_string(),
    }
}

fn convert_stop_reason_to_openai(reason: &str) -> String {
    match reason {
        "end_turn" => "stop".to_string(),
        "max_tokens" => "length".to_string(),
        "stop_sequence" => "stop".to_string(),
        _ => "stop".to_string(),
    }
}
