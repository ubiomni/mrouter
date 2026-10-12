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
        // NOTE: stream_options injection is handled by forwarder.rs based on
        // enable_stats/quota_group conditions. Do NOT inject here unconditionally.
    }

    // Copy stop_sequences → stop
    if let Some(stop) = body.get("stop_sequences") {
        result.insert("stop".to_string(), stop.clone());
    }

    // Convert thinking → reasoning_effort
    // Anthropic: {"thinking": {"type": "enabled", "budget_tokens": N}}
    // OpenAI: {"reasoning_effort": "low"|"medium"|"high"}
    if let Some(thinking) = body.get("thinking") {
        if thinking.get("type").and_then(|t| t.as_str()) == Some("enabled") {
            let budget = thinking.get("budget_tokens").and_then(|b| b.as_i64()).unwrap_or(0);
            let effort = if budget <= 4096 {
                "low"
            } else if budget <= 16384 {
                "medium"
            } else {
                "high"
            };
            result.insert("reasoning_effort".to_string(), json!(effort));
        }
    }

    // Convert tools: Anthropic → OpenAI
    // Anthropic: [{"name": "...", "description": "...", "input_schema": {...}}]
    // OpenAI: [{"type": "function", "function": {"name": "...", "description": "...", "parameters": {...}}}]
    if let Some(Value::Array(tools)) = body.get("tools") {
        let openai_tools: Vec<Value> = tools.iter().map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.get("name").cloned().unwrap_or(json!("")),
                    "description": tool.get("description").cloned().unwrap_or(json!("")),
                    "parameters": tool.get("input_schema").cloned().unwrap_or(json!({})),
                }
            })
        }).collect();
        result.insert("tools".to_string(), Value::Array(openai_tools));
    }

    // Convert tool_choice: Anthropic → OpenAI
    // Anthropic: {"type": "auto"} | {"type": "any"} | {"type": "tool", "name": "..."}
    // OpenAI: "auto" | "required" | {"type": "function", "function": {"name": "..."}}
    if let Some(tc) = body.get("tool_choice") {
        let openai_tc = match tc.get("type").and_then(|t| t.as_str()) {
            Some("auto") => json!("auto"),
            Some("any") => json!("required"),
            Some("tool") => {
                if let Some(name) = tc.get("name") {
                    json!({"type": "function", "function": {"name": name}})
                } else {
                    json!("auto")
                }
            }
            _ => json!("auto"),
        };
        result.insert("tool_choice".to_string(), openai_tc);
    }

    // Convert metadata.user_id → user
    if let Some(metadata) = body.get("metadata") {
        if let Some(user_id) = metadata.get("user_id").and_then(|u| u.as_str()) {
            result.insert("user".to_string(), json!(user_id));
        }
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

    match content {
        Value::String(_) => Some(json!({"role": role, "content": content})),
        Value::Array(blocks) => {
            // Separate text, tool_use, and tool_result blocks
            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();
            let mut tool_results = Vec::new();

            for block in blocks {
                match block.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            text_parts.push(text.to_string());
                        }
                    }
                    Some("tool_use") => {
                        tool_calls.push(json!({
                            "id": block.get("id").cloned().unwrap_or(json!("")),
                            "type": "function",
                            "function": {
                                "name": block.get("name").cloned().unwrap_or(json!("")),
                                "arguments": block.get("input")
                                    .map(|v| serde_json::to_string(v).unwrap_or_default())
                                    .unwrap_or_default(),
                            }
                        }));
                    }
                    Some("tool_result") => {
                        let content_text = block.get("content")
                            .and_then(|c| match c {
                                Value::String(s) => Some(s.clone()),
                                Value::Array(arr) => arr.iter()
                                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                                    .next()
                                    .map(|s| s.to_string()),
                                _ => None,
                            })
                            .unwrap_or_default();
                        tool_results.push(json!({
                            "role": "tool",
                            "tool_call_id": block.get("tool_use_id").cloned().unwrap_or(json!("")),
                            "content": content_text,
                        }));
                    }
                    _ => {
                        // thinking blocks etc — skip in OpenAI format
                    }
                }
            }

            // If this is a tool_result message, return tool messages
            if !tool_results.is_empty() {
                return if tool_results.len() == 1 {
                    Some(tool_results.into_iter().next().unwrap())
                } else {
                    // Multiple tool results — return first, caller should handle multi
                    Some(tool_results.into_iter().next().unwrap())
                };
            }

            // Build assistant message with optional tool_calls
            let mut result = serde_json::Map::new();
            result.insert("role".to_string(), json!(role));

            let combined_text = text_parts.join("\n");
            if !combined_text.is_empty() {
                result.insert("content".to_string(), json!(combined_text));
            } else if tool_calls.is_empty() {
                result.insert("content".to_string(), json!(""));
            }

            if !tool_calls.is_empty() {
                result.insert("tool_calls".to_string(), Value::Array(tool_calls));
            }

            Some(Value::Object(result))
        }
        _ => Some(json!({"role": role, "content": content})),
    }
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

    // Convert reasoning_effort → thinking
    // OpenAI: {"reasoning_effort": "low"|"medium"|"high"}
    // Anthropic: {"thinking": {"type": "enabled", "budget_tokens": N}}
    if let Some(effort) = body.get("reasoning_effort").and_then(|v| v.as_str()) {
        let budget = match effort {
            "low" => 4096,
            "medium" => 16384,
            _ => 32768, // "high" or unknown
        };
        result.insert("thinking".to_string(), json!({
            "type": "enabled",
            "budget_tokens": budget
        }));
    }

    // Convert tools: OpenAI → Anthropic
    // OpenAI: [{"type": "function", "function": {"name": "...", "description": "...", "parameters": {...}}}]
    // Anthropic: [{"name": "...", "description": "...", "input_schema": {...}}]
    if let Some(Value::Array(tools)) = body.get("tools") {
        let anthropic_tools: Vec<Value> = tools.iter().filter_map(|tool| {
            let func = tool.get("function")?;
            Some(json!({
                "name": func.get("name").cloned().unwrap_or(json!("")),
                "description": func.get("description").cloned().unwrap_or(json!("")),
                "input_schema": func.get("parameters").cloned().unwrap_or(json!({"type": "object", "properties": {}})),
            }))
        }).collect();
        result.insert("tools".to_string(), Value::Array(anthropic_tools));
    }

    // Convert tool_choice: OpenAI → Anthropic
    // OpenAI: "auto" | "none" | "required" | {"type": "function", "function": {"name": "..."}}
    // Anthropic: {"type": "auto"} | {"type": "any"} | {"type": "tool", "name": "..."}
    if let Some(tc) = body.get("tool_choice") {
        let anthropic_tc = match tc {
            Value::String(s) => match s.as_str() {
                "auto" => json!({"type": "auto"}),
                "required" => json!({"type": "any"}),
                "none" => json!({"type": "auto"}), // Anthropic has no "none", closest is auto
                _ => json!({"type": "auto"}),
            },
            Value::Object(_) => {
                // {"type": "function", "function": {"name": "..."}}
                if let Some(name) = tc.get("function").and_then(|f| f.get("name")) {
                    json!({"type": "tool", "name": name})
                } else {
                    json!({"type": "auto"})
                }
            }
            _ => json!({"type": "auto"}),
        };
        result.insert("tool_choice".to_string(), anthropic_tc);
    }

    // Convert user → metadata.user_id
    if let Some(user) = body.get("user").and_then(|u| u.as_str()) {
        result.insert("metadata".to_string(), json!({"user_id": user}));
    }

    (Value::Object(result), "/v1/messages".to_string())
}

fn convert_openai_msg_to_anthropic(msg: &Value) -> Option<Value> {
    let role = msg.get("role")?.as_str()?;
    let content = msg.get("content");

    match role {
        "tool" => {
            // OpenAI tool response → Anthropic tool_result block in user message
            let tool_call_id = msg.get("tool_call_id").and_then(|t| t.as_str()).unwrap_or("");
            let content_text = content.and_then(|c| c.as_str()).unwrap_or("");
            Some(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": tool_call_id,
                    "content": content_text,
                }]
            }))
        }
        "assistant" => {
            let mut blocks = Vec::new();

            // Add text content
            if let Some(c) = content {
                match c {
                    Value::String(s) if !s.is_empty() => {
                        blocks.push(json!({"type": "text", "text": s}));
                    }
                    _ => {}
                }
            }

            // Convert tool_calls → tool_use blocks
            if let Some(Value::Array(tool_calls)) = msg.get("tool_calls") {
                for tc in tool_calls {
                    if let Some(func) = tc.get("function") {
                        let args_str = func.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}");
                        let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.get("id").cloned().unwrap_or(json!("")),
                            "name": func.get("name").cloned().unwrap_or(json!("")),
                            "input": input,
                        }));
                    }
                }
            }

            if blocks.is_empty() {
                blocks.push(json!({"type": "text", "text": ""}));
            }

            Some(json!({"role": "assistant", "content": blocks}))
        }
        _ => {
            // user / other roles
            let anthropic_content = match content {
                Some(Value::String(s)) => json!([{"type": "text", "text": s}]),
                Some(other) => other.clone(),
                None => json!([{"type": "text", "text": ""}]),
            };
            Some(json!({"role": role, "content": anthropic_content}))
        }
    }
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
        // No choices — check if this is a usage-only chunk (from stream_options.include_usage)
        // If so, emit a message_delta with the usage data (Anthropic expects usage in message_delta)
        if let Some(usage) = chunk.get("usage") {
            let input = usage.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
            let output = usage.get("completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
            let msg_delta = json!({
                "type": "message_delta",
                "delta": {},
                "usage": {"input_tokens": input, "output_tokens": output}
            });
            return format!(
                "event: message_delta\ndata: {}\n\n",
                serde_json::to_string(&msg_delta).unwrap_or_default()
            );
        }
        // No choices, no usage — emit ping
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

            // Forward usage if present — map Anthropic field names to OpenAI
            if let Some(usage) = event.get("usage") {
                let input = usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                let output = usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                chunk["usage"] = json!({
                    "prompt_tokens": input,
                    "completion_tokens": output,
                    "total_tokens": input + output,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ─── Thinking / Reasoning ─────────────────────────────────────────────

    #[test]
    fn test_anthropic_thinking_to_openai_reasoning_effort() {
        let body = json!({
            "model": "claude-3-opus",
            "messages": [{"role": "user", "content": "hello"}],
            "max_tokens": 1024,
            "thinking": {"type": "enabled", "budget_tokens": 20000}
        });
        let (result, _) = convert_request(ApiFormat::Anthropic, ApiFormat::OpenAI, &body, "/v1/messages");
        assert_eq!(result["reasoning_effort"], "high");
    }

    #[test]
    fn test_anthropic_thinking_low_budget() {
        let body = json!({
            "model": "claude-3",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 512,
            "thinking": {"type": "enabled", "budget_tokens": 2000}
        });
        let (result, _) = convert_request(ApiFormat::Anthropic, ApiFormat::OpenAI, &body, "/v1/messages");
        assert_eq!(result["reasoning_effort"], "low");
    }

    #[test]
    fn test_anthropic_thinking_medium_budget() {
        let body = json!({
            "model": "claude-3",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 512,
            "thinking": {"type": "enabled", "budget_tokens": 10000}
        });
        let (result, _) = convert_request(ApiFormat::Anthropic, ApiFormat::OpenAI, &body, "/v1/messages");
        assert_eq!(result["reasoning_effort"], "medium");
    }

    #[test]
    fn test_openai_reasoning_effort_to_anthropic_thinking() {
        let body = json!({
            "model": "o1",
            "messages": [{"role": "user", "content": "solve this"}],
            "reasoning_effort": "high"
        });
        let (result, _) = convert_request(ApiFormat::OpenAI, ApiFormat::Anthropic, &body, "/v1/chat/completions");
        assert_eq!(result["thinking"]["type"], "enabled");
        assert_eq!(result["thinking"]["budget_tokens"], 32768);
    }

    #[test]
    fn test_openai_reasoning_effort_low() {
        let body = json!({
            "model": "o1",
            "messages": [{"role": "user", "content": "quick"}],
            "reasoning_effort": "low"
        });
        let (result, _) = convert_request(ApiFormat::OpenAI, ApiFormat::Anthropic, &body, "/v1/chat/completions");
        assert_eq!(result["thinking"]["budget_tokens"], 4096);
    }

    // ─── Tools: Anthropic → OpenAI ────────────────────────────────────────

    #[test]
    fn test_anthropic_tools_to_openai() {
        let body = json!({
            "model": "claude-3",
            "messages": [{"role": "user", "content": "get weather"}],
            "max_tokens": 1024,
            "tools": [{
                "name": "get_weather",
                "description": "Get weather for a location",
                "input_schema": {
                    "type": "object",
                    "properties": {"location": {"type": "string"}},
                    "required": ["location"]
                }
            }]
        });
        let (result, _) = convert_request(ApiFormat::Anthropic, ApiFormat::OpenAI, &body, "/v1/messages");
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "get_weather");
        assert_eq!(tools[0]["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn test_anthropic_tool_choice_any_to_openai() {
        let body = json!({
            "model": "claude-3",
            "messages": [{"role": "user", "content": "do it"}],
            "max_tokens": 1024,
            "tool_choice": {"type": "any"}
        });
        let (result, _) = convert_request(ApiFormat::Anthropic, ApiFormat::OpenAI, &body, "/v1/messages");
        assert_eq!(result["tool_choice"], "required");
    }

    #[test]
    fn test_anthropic_tool_choice_specific_to_openai() {
        let body = json!({
            "model": "claude-3",
            "messages": [{"role": "user", "content": "do it"}],
            "max_tokens": 1024,
            "tool_choice": {"type": "tool", "name": "get_weather"}
        });
        let (result, _) = convert_request(ApiFormat::Anthropic, ApiFormat::OpenAI, &body, "/v1/messages");
        assert_eq!(result["tool_choice"]["type"], "function");
        assert_eq!(result["tool_choice"]["function"]["name"], "get_weather");
    }

    // ─── Tools: OpenAI → Anthropic ────────────────────────────────────────

    #[test]
    fn test_openai_tools_to_anthropic() {
        let body = json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "get weather"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get weather",
                    "parameters": {
                        "type": "object",
                        "properties": {"city": {"type": "string"}}
                    }
                }
            }]
        });
        let (result, _) = convert_request(ApiFormat::OpenAI, ApiFormat::Anthropic, &body, "/v1/chat/completions");
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "get_weather");
        assert_eq!(tools[0]["input_schema"]["type"], "object");
    }

    #[test]
    fn test_openai_tool_choice_required_to_anthropic() {
        let body = json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "do it"}],
            "tool_choice": "required"
        });
        let (result, _) = convert_request(ApiFormat::OpenAI, ApiFormat::Anthropic, &body, "/v1/chat/completions");
        assert_eq!(result["tool_choice"]["type"], "any");
    }

    #[test]
    fn test_openai_tool_choice_specific_to_anthropic() {
        let body = json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "do it"}],
            "tool_choice": {"type": "function", "function": {"name": "search"}}
        });
        let (result, _) = convert_request(ApiFormat::OpenAI, ApiFormat::Anthropic, &body, "/v1/chat/completions");
        assert_eq!(result["tool_choice"]["type"], "tool");
        assert_eq!(result["tool_choice"]["name"], "search");
    }

    // ─── Messages with tool_use / tool_result ─────────────────────────────

    #[test]
    fn test_anthropic_tool_use_msg_to_openai() {
        let body = json!({
            "model": "claude-3",
            "messages": [
                {"role": "user", "content": "get weather in tokyo"},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "Let me check"},
                    {"type": "tool_use", "id": "call_123", "name": "get_weather", "input": {"location": "tokyo"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "call_123", "content": "sunny 25C"}
                ]}
            ],
            "max_tokens": 1024
        });
        let (result, _) = convert_request(ApiFormat::Anthropic, ApiFormat::OpenAI, &body, "/v1/messages");
        let msgs = result["messages"].as_array().unwrap();
        // assistant message should have tool_calls
        let assistant = &msgs[1];
        assert_eq!(assistant["tool_calls"][0]["id"], "call_123");
        assert_eq!(assistant["tool_calls"][0]["function"]["name"], "get_weather");
        // tool_result → tool role
        let tool_msg = &msgs[2];
        assert_eq!(tool_msg["role"], "tool");
        assert_eq!(tool_msg["tool_call_id"], "call_123");
        assert_eq!(tool_msg["content"], "sunny 25C");
    }

    #[test]
    fn test_openai_tool_calls_msg_to_anthropic() {
        let body = json!({
            "model": "gpt-4",
            "messages": [
                {"role": "user", "content": "get weather"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": "call_abc",
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{\"city\":\"paris\"}"}
                }]},
                {"role": "tool", "tool_call_id": "call_abc", "content": "rainy 15C"}
            ]
        });
        let (result, _) = convert_request(ApiFormat::OpenAI, ApiFormat::Anthropic, &body, "/v1/chat/completions");
        let msgs = result["messages"].as_array().unwrap();
        // assistant with tool_use
        let assistant = &msgs[1];
        assert_eq!(assistant["role"], "assistant");
        let blocks = assistant["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "tool_use");
        assert_eq!(blocks[0]["name"], "get_weather");
        assert_eq!(blocks[0]["input"]["city"], "paris");
        // tool → user with tool_result
        let tool_result = &msgs[2];
        assert_eq!(tool_result["role"], "user");
        assert_eq!(tool_result["content"][0]["type"], "tool_result");
        assert_eq!(tool_result["content"][0]["tool_use_id"], "call_abc");
    }

    // ─── Metadata / User ──────────────────────────────────────────────────

    #[test]
    fn test_anthropic_metadata_to_openai_user() {
        let body = json!({
            "model": "claude-3",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 100,
            "metadata": {"user_id": "user_123"}
        });
        let (result, _) = convert_request(ApiFormat::Anthropic, ApiFormat::OpenAI, &body, "/v1/messages");
        assert_eq!(result["user"], "user_123");
    }

    #[test]
    fn test_openai_user_to_anthropic_metadata() {
        let body = json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "hi"}],
            "user": "user_456"
        });
        let (result, _) = convert_request(ApiFormat::OpenAI, ApiFormat::Anthropic, &body, "/v1/chat/completions");
        assert_eq!(result["metadata"]["user_id"], "user_456");
    }

    // ─── Basic conversion correctness ─────────────────────────────────────

    #[test]
    fn test_anthropic_to_openai_basic() {
        let body = json!({
            "model": "claude-3-sonnet",
            "system": "You are helpful",
            "messages": [{"role": "user", "content": "hello"}],
            "max_tokens": 2048,
            "temperature": 0.7,
            "stream": true,
            "stop_sequences": ["END"]
        });
        let (result, path) = convert_request(ApiFormat::Anthropic, ApiFormat::OpenAI, &body, "/v1/messages");
        assert_eq!(path, "/v1/chat/completions");
        assert_eq!(result["model"], "claude-3-sonnet");
        assert_eq!(result["temperature"], 0.7);
        assert_eq!(result["stream"], true);
        assert_eq!(result["stop"], json!(["END"]));
        assert_eq!(result["max_tokens"], 2048);
        // System should be first message
        let msgs = result["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are helpful");
    }

    #[test]
    fn test_openai_to_anthropic_basic() {
        let body = json!({
            "model": "gpt-4",
            "messages": [
                {"role": "system", "content": "Be concise"},
                {"role": "user", "content": "hi"}
            ],
            "max_completion_tokens": 4000,
            "temperature": 0.5,
            "stop": ["DONE"]
        });
        let (result, path) = convert_request(ApiFormat::OpenAI, ApiFormat::Anthropic, &body, "/v1/chat/completions");
        assert_eq!(path, "/v1/messages");
        assert_eq!(result["system"], "Be concise");
        assert_eq!(result["max_tokens"], 4000);
        assert_eq!(result["temperature"], 0.5);
        assert_eq!(result["stop_sequences"], json!(["DONE"]));
        let msgs = result["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn test_same_format_passthrough() {
        let body = json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "test"}],
            "custom_field": "should_preserve"
        });
        let (result, path) = convert_request(ApiFormat::OpenAI, ApiFormat::OpenAI, &body, "/v1/chat/completions");
        assert_eq!(path, "/v1/chat/completions");
        assert_eq!(result, body);
    }

    // ─── Response conversion ──────────────────────────────────────────────

    #[test]
    fn test_openai_response_to_anthropic() {
        let body = json!({
            "id": "chatcmpl-123",
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        });
        let result = convert_response(ApiFormat::Anthropic, ApiFormat::OpenAI, &body);
        assert_eq!(result["type"], "message");
        assert_eq!(result["role"], "assistant");
        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "Hello!");
        assert_eq!(result["stop_reason"], "end_turn");
        assert_eq!(result["usage"]["input_tokens"], 10);
        assert_eq!(result["usage"]["output_tokens"], 5);
    }

    #[test]
    fn test_anthropic_response_to_openai() {
        let body = json!({
            "id": "msg_abc123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "World!"}],
            "model": "claude-3-sonnet",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 20, "output_tokens": 8}
        });
        let result = convert_response(ApiFormat::OpenAI, ApiFormat::Anthropic, &body);
        assert_eq!(result["object"], "chat.completion");
        assert_eq!(result["choices"][0]["message"]["content"], "World!");
        assert_eq!(result["choices"][0]["finish_reason"], "stop");
        assert_eq!(result["usage"]["prompt_tokens"], 20);
        assert_eq!(result["usage"]["completion_tokens"], 8);
        assert_eq!(result["usage"]["total_tokens"], 28);
    }

    #[test]
    fn test_anthropic_max_tokens_response_to_openai() {
        let body = json!({
            "id": "msg_abc",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "partial..."}],
            "stop_reason": "max_tokens",
            "usage": {"input_tokens": 5, "output_tokens": 100}
        });
        let result = convert_response(ApiFormat::OpenAI, ApiFormat::Anthropic, &body);
        assert_eq!(result["choices"][0]["finish_reason"], "length");
    }

    // ─── Edge cases ───────────────────────────────────────────────────────

    #[test]
    fn test_anthropic_system_as_array() {
        let body = json!({
            "model": "claude-3",
            "system": [
                {"type": "text", "text": "First instruction"},
                {"type": "text", "text": "Second instruction"}
            ],
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 100
        });
        let (result, _) = convert_request(ApiFormat::Anthropic, ApiFormat::OpenAI, &body, "/v1/messages");
        let msgs = result["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert!(msgs[0]["content"].as_str().unwrap().contains("First instruction"));
        assert!(msgs[0]["content"].as_str().unwrap().contains("Second instruction"));
    }

    #[test]
    fn test_openai_multiple_system_messages() {
        let body = json!({
            "model": "gpt-4",
            "messages": [
                {"role": "system", "content": "Rule 1"},
                {"role": "system", "content": "Rule 2"},
                {"role": "user", "content": "hello"}
            ]
        });
        let (result, _) = convert_request(ApiFormat::OpenAI, ApiFormat::Anthropic, &body, "/v1/chat/completions");
        assert_eq!(result["system"], "Rule 1\nRule 2");
        let msgs = result["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn test_openai_to_anthropic_no_max_tokens_defaults() {
        let body = json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "test"}]
        });
        let (result, _) = convert_request(ApiFormat::OpenAI, ApiFormat::Anthropic, &body, "/v1/chat/completions");
        // Anthropic requires max_tokens, should default to 4096
        assert_eq!(result["max_tokens"], 4096);
    }

    #[test]
    fn test_multiple_tools_conversion() {
        let body = json!({
            "model": "claude-3",
            "messages": [{"role": "user", "content": "help"}],
            "max_tokens": 1024,
            "tools": [
                {"name": "search", "description": "Search web", "input_schema": {"type": "object", "properties": {"q": {"type": "string"}}}},
                {"name": "calc", "description": "Calculate", "input_schema": {"type": "object", "properties": {"expr": {"type": "string"}}}}
            ],
            "tool_choice": {"type": "auto"}
        });
        let (result, _) = convert_request(ApiFormat::Anthropic, ApiFormat::OpenAI, &body, "/v1/messages");
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["function"]["name"], "search");
        assert_eq!(tools[1]["function"]["name"], "calc");
        assert_eq!(result["tool_choice"], "auto");
    }

    #[test]
    fn test_roundtrip_tools_openai_anthropic_openai() {
        // OpenAI → Anthropic → OpenAI should preserve tool semantics
        let original = json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "test"}],
            "tools": [{
                "type": "function",
                "function": {"name": "fetch", "description": "Fetch URL", "parameters": {"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}}
            }],
            "tool_choice": "required"
        });
        let (anthropic, _) = convert_request(ApiFormat::OpenAI, ApiFormat::Anthropic, &original, "/v1/chat/completions");
        assert_eq!(anthropic["tools"][0]["name"], "fetch");
        assert_eq!(anthropic["tool_choice"]["type"], "any");

        let (back, _) = convert_request(ApiFormat::Anthropic, ApiFormat::OpenAI, &anthropic, "/v1/messages");
        assert_eq!(back["tools"][0]["function"]["name"], "fetch");
        assert_eq!(back["tool_choice"], "required");
    }
}
