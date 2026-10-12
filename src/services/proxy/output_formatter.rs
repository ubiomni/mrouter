// Output formatters — render normalized StreamEvents into client-facing SSE format

use serde_json::json;
use crate::models::ApiFormat;
use super::stream_event::{StreamEvent, StreamUsage, StopReason};

/// Trait for formatting StreamEvents into client-facing SSE text.
pub trait OutputFormatter: Send + Sync {
    /// Format a single StreamEvent as SSE text (including "data: " prefix and trailing newlines).
    /// Returns empty string if the event should be suppressed.
    fn format_event(&self, event: &StreamEvent) -> String;
}

/// Get the appropriate output formatter for a client format.
pub fn get_output_formatter(format: ApiFormat) -> Box<dyn OutputFormatter> {
    match format {
        ApiFormat::OpenAI => Box::new(OpenAIOutputFormatter),
        ApiFormat::OpenAIResponses => Box::new(ResponsesApiOutputFormatter),
        ApiFormat::Anthropic => Box::new(AnthropicOutputFormatter),
        ApiFormat::Google => Box::new(GoogleOutputFormatter),
        ApiFormat::Passthrough => Box::new(OpenAIOutputFormatter),
    }
}

// ─── OpenAI Output ─────────────────────────────────────────────────────

pub struct OpenAIOutputFormatter;

impl OutputFormatter for OpenAIOutputFormatter {
    fn format_event(&self, event: &StreamEvent) -> String {
        match event {
            StreamEvent::MessageStart { id, model } => {
                let chunk = json!({
                    "id": id,
                    "object": "chat.completion.chunk",
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": {"role": "assistant"},
                        "finish_reason": null
                    }]
                });
                format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default())
            }

            StreamEvent::ContentDelta { index, text } => {
                let chunk = json!({
                    "id": "chatcmpl-stream",
                    "object": "chat.completion.chunk",
                    "choices": [{
                        "index": index,
                        "delta": {"content": text},
                        "finish_reason": null
                    }]
                });
                format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default())
            }

            StreamEvent::ThinkingDelta { index, text } => {
                // Emit as reasoning_content (OpenAI o-series / DeepSeek convention)
                let chunk = json!({
                    "id": "chatcmpl-stream",
                    "object": "chat.completion.chunk",
                    "choices": [{
                        "index": index,
                        "delta": {"reasoning_content": text},
                        "finish_reason": null
                    }]
                });
                format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default())
            }

            StreamEvent::ToolCallDelta { index, tool_call_id, name, arguments } => {
                let mut tc = json!({
                    "index": index,
                    "function": {"arguments": arguments}
                });
                if !tool_call_id.is_empty() {
                    tc["id"] = json!(tool_call_id);
                    tc["type"] = json!("function");
                }
                if let Some(n) = name {
                    tc["function"]["name"] = json!(n);
                }
                let chunk = json!({
                    "id": "chatcmpl-stream",
                    "object": "chat.completion.chunk",
                    "choices": [{
                        "index": 0,
                        "delta": {"tool_calls": [tc]},
                        "finish_reason": null
                    }]
                });
                format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default())
            }

            StreamEvent::ContentStop { .. } => String::new(),

            StreamEvent::MessageDelta { stop_reason, usage } => {
                let mut chunk = json!({
                    "id": "chatcmpl-stream",
                    "object": "chat.completion.chunk",
                    "choices": [{
                        "index": 0,
                        "delta": {},
                        "finish_reason": stop_reason.to_openai()
                    }]
                });
                if let Some(u) = usage {
                    chunk["usage"] = openai_usage_json(u);
                }
                format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default())
            }

            StreamEvent::MessageStop => {
                "data: [DONE]\n\n".to_string()
            }

            StreamEvent::UsageUpdate(u) => {
                // Emit usage as a standalone chunk (from stream_options.include_usage)
                let chunk = json!({
                    "id": "chatcmpl-stream",
                    "object": "chat.completion.chunk",
                    "choices": [],
                    "usage": openai_usage_json(u)
                });
                format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default())
            }

            StreamEvent::Ping => String::new(),
        }
    }
}

// ─── Anthropic Output ──────────────────────────────────────────────────

pub struct AnthropicOutputFormatter;

impl OutputFormatter for AnthropicOutputFormatter {
    fn format_event(&self, event: &StreamEvent) -> String {
        match event {
            StreamEvent::MessageStart { id, model } => {
                let msg_start = json!({
                    "type": "message_start",
                    "message": {
                        "id": id,
                        "type": "message",
                        "role": "assistant",
                        "content": [],
                        "model": model,
                        "stop_reason": null,
                        "usage": {"input_tokens": 0, "output_tokens": 0}
                    }
                });
                // Also emit the first content_block_start (text block at index 0)
                let block_start = json!({
                    "type": "content_block_start",
                    "index": 0,
                    "content_block": {"type": "text", "text": ""}
                });
                format!(
                    "event: message_start\ndata: {}\n\nevent: content_block_start\ndata: {}\n\n",
                    serde_json::to_string(&msg_start).unwrap_or_default(),
                    serde_json::to_string(&block_start).unwrap_or_default(),
                )
            }

            StreamEvent::ContentDelta { index, text } => {
                let block_delta = json!({
                    "type": "content_block_delta",
                    "index": index,
                    "delta": {"type": "text_delta", "text": text}
                });
                format!(
                    "event: content_block_delta\ndata: {}\n\n",
                    serde_json::to_string(&block_delta).unwrap_or_default()
                )
            }

            StreamEvent::ThinkingDelta { index, text } => {
                let block_delta = json!({
                    "type": "content_block_delta",
                    "index": index,
                    "delta": {"type": "thinking_delta", "thinking": text}
                });
                format!(
                    "event: content_block_delta\ndata: {}\n\n",
                    serde_json::to_string(&block_delta).unwrap_or_default()
                )
            }

            StreamEvent::ToolCallDelta { index, tool_call_id, name, arguments } => {
                let mut result = String::new();
                // If we have a name, this is the start of a tool_use block
                if name.is_some() {
                    let block = json!({
                        "type": "content_block_start",
                        "index": index,
                        "content_block": {"type": "tool_use", "id": tool_call_id, "name": name, "input": {}}
                    });
                    result.push_str(&format!(
                        "event: content_block_start\ndata: {}\n\n",
                        serde_json::to_string(&block).unwrap_or_default()
                    ));
                }

                if !arguments.is_empty() {
                    let delta = json!({
                        "type": "content_block_delta",
                        "index": index,
                        "delta": {"type": "input_json_delta", "partial_json": arguments}
                    });
                    result.push_str(&format!(
                        "event: content_block_delta\ndata: {}\n\n",
                        serde_json::to_string(&delta).unwrap_or_default()
                    ));
                }
                result
            }

            StreamEvent::ContentStop { index } => {
                let block_stop = json!({"type": "content_block_stop", "index": index});
                format!(
                    "event: content_block_stop\ndata: {}\n\n",
                    serde_json::to_string(&block_stop).unwrap_or_default()
                )
            }

            StreamEvent::MessageDelta { stop_reason, usage } => {
                // Emit content_block_stop for the last block, then message_delta
                let block_stop = json!({"type": "content_block_stop", "index": 0});
                let mut msg_delta = json!({
                    "type": "message_delta",
                    "delta": {"stop_reason": stop_reason.to_anthropic()}
                });
                if let Some(u) = usage {
                    msg_delta["usage"] = json!({"output_tokens": u.output_tokens});
                }
                format!(
                    "event: content_block_stop\ndata: {}\n\nevent: message_delta\ndata: {}\n\n",
                    serde_json::to_string(&block_stop).unwrap_or_default(),
                    serde_json::to_string(&msg_delta).unwrap_or_default(),
                )
            }

            StreamEvent::MessageStop => {
                let msg_stop = json!({"type": "message_stop"});
                format!(
                    "event: message_stop\ndata: {}\n\n",
                    serde_json::to_string(&msg_stop).unwrap_or_default()
                )
            }

            StreamEvent::UsageUpdate(u) => {
                // Emit as message_delta with usage (Anthropic SSE convention)
                let msg_delta = json!({
                    "type": "message_delta",
                    "delta": {},
                    "usage": {"input_tokens": u.input_tokens, "output_tokens": u.output_tokens}
                });
                format!(
                    "event: message_delta\ndata: {}\n\n",
                    serde_json::to_string(&msg_delta).unwrap_or_default()
                )
            }

            StreamEvent::Ping => String::new(),
        }
    }
}

// ─── Google Output ─────────────────────────────────────────────────────

pub struct GoogleOutputFormatter;

impl OutputFormatter for GoogleOutputFormatter {
    fn format_event(&self, event: &StreamEvent) -> String {
        match event {
            StreamEvent::ContentDelta { text, .. } => {
                let chunk = json!({
                    "candidates": [{
                        "content": {"parts": [{"text": text}], "role": "model"},
                        "index": 0
                    }]
                });
                format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default())
            }

            StreamEvent::ThinkingDelta { text, .. } => {
                let chunk = json!({
                    "candidates": [{
                        "content": {"parts": [{"thought": text}], "role": "model"},
                        "index": 0
                    }]
                });
                format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default())
            }

            StreamEvent::MessageDelta { stop_reason, usage } => {
                let finish = match stop_reason {
                    StopReason::EndTurn | StopReason::StopSequence => "STOP",
                    StopReason::MaxTokens => "MAX_TOKENS",
                    StopReason::ContentFilter => "SAFETY",
                    StopReason::ToolUse => "STOP",
                };
                let mut chunk = json!({
                    "candidates": [{
                        "finishReason": finish,
                        "index": 0
                    }]
                });
                if let Some(u) = usage {
                    chunk["usageMetadata"] = json!({
                        "promptTokenCount": u.input_tokens,
                        "candidatesTokenCount": u.output_tokens,
                        "totalTokenCount": u.input_tokens + u.output_tokens
                    });
                }
                format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default())
            }

            StreamEvent::MessageStop => String::new(),
            StreamEvent::MessageStart { .. } => String::new(),
            StreamEvent::ContentStop { .. } => String::new(),
            StreamEvent::ToolCallDelta { .. } => String::new(), // TODO: Gemini function calling

            StreamEvent::UsageUpdate(u) => {
                let chunk = json!({
                    "usageMetadata": {
                        "promptTokenCount": u.input_tokens,
                        "candidatesTokenCount": u.output_tokens,
                        "totalTokenCount": u.input_tokens + u.output_tokens
                    }
                });
                format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default())
            }

            StreamEvent::Ping => String::new(),
        }
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────

fn openai_usage_json(u: &StreamUsage) -> serde_json::Value {
    json!({
        "prompt_tokens": u.input_tokens,
        "completion_tokens": u.output_tokens,
        "total_tokens": u.input_tokens + u.output_tokens
    })
}

// ─── OpenAI Responses API Output ───────────────────────────────────────

pub struct ResponsesApiOutputFormatter;

impl OutputFormatter for ResponsesApiOutputFormatter {
    fn format_event(&self, event: &StreamEvent) -> String {
        match event {
            StreamEvent::MessageStart { id, model } => {
                let payload = json!({
                    "type": "response.created",
                    "response": {
                        "id": id,
                        "object": "response",
                        "status": "in_progress",
                        "model": model,
                        "output": [],
                        "usage": null
                    }
                });
                format_responses_sse("response.created", &payload)
            }

            StreamEvent::ContentDelta { index, text } => {
                let payload = json!({
                    "type": "response.output_text.delta",
                    "output_index": 0,
                    "content_index": index,
                    "delta": text
                });
                format_responses_sse("response.output_text.delta", &payload)
            }

            StreamEvent::ThinkingDelta { index, text } => {
                let payload = json!({
                    "type": "response.reasoning_summary_text.delta",
                    "summary_index": index,
                    "delta": text
                });
                format_responses_sse("response.reasoning_summary_text.delta", &payload)
            }

            StreamEvent::ToolCallDelta { index, tool_call_id, name, arguments } => {
                let mut result = String::new();

                // If name is present, emit output_item.added first
                if let Some(n) = name {
                    let item_added = json!({
                        "type": "response.output_item.added",
                        "output_index": index,
                        "item": {
                            "type": "function_call",
                            "id": tool_call_id,
                            "call_id": tool_call_id,
                            "name": n,
                            "arguments": "",
                            "status": "in_progress"
                        }
                    });
                    result.push_str(&format_responses_sse("response.output_item.added", &item_added));
                }

                if !arguments.is_empty() {
                    let delta = json!({
                        "type": "response.function_call_arguments.delta",
                        "output_index": index,
                        "item_id": tool_call_id,
                        "delta": arguments
                    });
                    result.push_str(&format_responses_sse("response.function_call_arguments.delta", &delta));
                }
                result
            }

            StreamEvent::ContentStop { index } => {
                let payload = json!({
                    "type": "response.output_text.done",
                    "output_index": 0,
                    "content_index": index,
                    "text": ""
                });
                format_responses_sse("response.output_text.done", &payload)
            }

            StreamEvent::MessageDelta { stop_reason, usage } => {
                let status = match stop_reason {
                    StopReason::EndTurn | StopReason::StopSequence => "completed",
                    StopReason::MaxTokens => "incomplete",
                    StopReason::ContentFilter => "failed",
                    StopReason::ToolUse => "completed",
                };
                let mut response = json!({
                    "id": "resp_stream",
                    "object": "response",
                    "status": status,
                    "output": []
                });
                if let Some(u) = usage {
                    response["usage"] = json!({
                        "input_tokens": u.input_tokens,
                        "output_tokens": u.output_tokens,
                        "total_tokens": u.input_tokens + u.output_tokens
                    });
                }
                let payload = json!({
                    "type": "response.completed",
                    "response": response
                });
                format_responses_sse("response.completed", &payload)
            }

            StreamEvent::MessageStop => String::new(),

            StreamEvent::UsageUpdate(u) => {
                // Emit as response.completed with usage
                let response = json!({
                    "id": "resp_stream",
                    "object": "response",
                    "status": "completed",
                    "output": [],
                    "usage": {
                        "input_tokens": u.input_tokens,
                        "output_tokens": u.output_tokens,
                        "total_tokens": u.input_tokens + u.output_tokens
                    }
                });
                let payload = json!({"type": "response.completed", "response": response});
                format_responses_sse("response.completed", &payload)
            }

            StreamEvent::Ping => String::new(),
        }
    }
}

fn format_responses_sse(event_type: &str, data: &serde_json::Value) -> String {
    format!(
        "event: {}\ndata: {}\n\n",
        event_type,
        serde_json::to_string(data).unwrap_or_default()
    )
}
