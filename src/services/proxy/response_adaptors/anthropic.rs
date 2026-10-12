// Anthropic response adaptor — parses event-typed SSE (message_start, content_block_delta, etc.)

use serde_json::Value;
use super::ResponseAdaptor;
use crate::services::proxy::stream_event::{StreamEvent, StreamUsage, StopReason};

pub struct AnthropicResponseAdaptor;

impl ResponseAdaptor for AnthropicResponseAdaptor {
    fn parse_sse_data(&self, data: &str) -> Vec<StreamEvent> {
        let data = data.trim();
        if data.is_empty() {
            return vec![];
        }

        let Ok(json) = serde_json::from_str::<Value>(data) else {
            return vec![];
        };

        let event_type = json.get("type").and_then(|t| t.as_str()).unwrap_or("");
        self.parse_typed_event(event_type, &json)
    }

    fn parse_sse_line(&self, event_type: Option<&str>, data: &str) -> Vec<StreamEvent> {
        let data = data.trim();
        if data.is_empty() {
            return vec![];
        }

        let Ok(json) = serde_json::from_str::<Value>(data) else {
            return vec![];
        };

        // Use explicit event type if provided, otherwise fall back to type field in JSON
        let etype = event_type
            .or_else(|| json.get("type").and_then(|t| t.as_str()))
            .unwrap_or("");

        self.parse_typed_event(etype, &json)
    }

    fn parse_response(&self, body: &Value) -> Vec<StreamEvent> {
        let mut events = Vec::new();

        let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("msg").to_string();
        let model = body.get("model").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        events.push(StreamEvent::MessageStart { id, model });

        // Parse content blocks
        if let Some(Value::Array(content)) = body.get("content") {
            for (i, block) in content.iter().enumerate() {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                let index = i as u32;

                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            events.push(StreamEvent::ContentDelta { index, text: text.to_string() });
                        }
                    }
                    "thinking" => {
                        if let Some(text) = block.get("thinking").and_then(|t| t.as_str()) {
                            events.push(StreamEvent::ThinkingDelta { index, text: text.to_string() });
                        }
                    }
                    "tool_use" => {
                        let tool_id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let name = block.get("name").and_then(|n| n.as_str()).map(String::from);
                        let args = block.get("input")
                            .map(|v| serde_json::to_string(v).unwrap_or_default())
                            .unwrap_or_default();
                        events.push(StreamEvent::ToolCallDelta {
                            index,
                            tool_call_id: tool_id,
                            name,
                            arguments: args,
                        });
                    }
                    _ => {}
                }
                events.push(StreamEvent::ContentStop { index });
            }
        }

        let stop_reason = body.get("stop_reason")
            .and_then(|r| r.as_str())
            .unwrap_or("end_turn");
        let usage = body.get("usage").map(|u| parse_anthropic_usage(u));
        events.push(StreamEvent::MessageDelta { stop_reason: StopReason::from_anthropic(stop_reason), usage });
        events.push(StreamEvent::MessageStop);
        events
    }
}

impl AnthropicResponseAdaptor {
    fn parse_typed_event(&self, event_type: &str, json: &Value) -> Vec<StreamEvent> {
        match event_type {
            "message_start" => {
                let msg = json.get("message").unwrap_or(json);
                let id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("msg").to_string();
                let model = msg.get("model").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();

                let mut events = vec![StreamEvent::MessageStart { id, model }];

                // Extract input usage from message_start
                if let Some(usage) = msg.get("usage") {
                    let u = parse_anthropic_usage(usage);
                    if u.input_tokens > 0 {
                        events.push(StreamEvent::UsageUpdate(u));
                    }
                }
                events
            }

            "content_block_start" => {
                // We don't emit a separate event for block start — the first delta carries the content
                vec![]
            }

            "content_block_delta" => {
                let index = json.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                let delta = json.get("delta");
                let delta_type = delta.and_then(|d| d.get("type")).and_then(|t| t.as_str()).unwrap_or("");

                match delta_type {
                    "text_delta" => {
                        let text = delta.and_then(|d| d.get("text")).and_then(|t| t.as_str()).unwrap_or("");
                        if text.is_empty() {
                            vec![]
                        } else {
                            vec![StreamEvent::ContentDelta { index, text: text.to_string() }]
                        }
                    }
                    "thinking_delta" => {
                        let text = delta.and_then(|d| d.get("thinking")).and_then(|t| t.as_str()).unwrap_or("");
                        if text.is_empty() {
                            vec![]
                        } else {
                            vec![StreamEvent::ThinkingDelta { index, text: text.to_string() }]
                        }
                    }
                    "input_json_delta" => {
                        let partial = delta.and_then(|d| d.get("partial_json")).and_then(|t| t.as_str()).unwrap_or("");
                        vec![StreamEvent::ToolCallDelta {
                            index,
                            tool_call_id: String::new(),
                            name: None,
                            arguments: partial.to_string(),
                        }]
                    }
                    _ => vec![]
                }
            }

            "content_block_stop" => {
                let index = json.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                vec![StreamEvent::ContentStop { index }]
            }

            "message_delta" => {
                let delta = json.get("delta");
                let stop_reason = delta
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(|r| r.as_str())
                    .unwrap_or("end_turn");
                let usage = json.get("usage").map(|u| parse_anthropic_usage(u));
                vec![StreamEvent::MessageDelta {
                    stop_reason: StopReason::from_anthropic(stop_reason),
                    usage,
                }]
            }

            "message_stop" => {
                vec![StreamEvent::MessageStop]
            }

            "ping" => {
                vec![StreamEvent::Ping]
            }

            "error" => {
                // Pass through as ping — error handling is done at a higher level
                vec![StreamEvent::Ping]
            }

            _ => vec![]
        }
    }
}

fn parse_anthropic_usage(usage: &Value) -> StreamUsage {
    StreamUsage {
        input_tokens: usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        output_tokens: usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        cache_read_tokens: usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        cache_creation_tokens: usage.get("cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
    }
}
