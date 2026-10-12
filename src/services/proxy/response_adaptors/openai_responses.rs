// OpenAI Responses API adaptor — parses /v1/responses streaming events

use serde_json::Value;
use super::ResponseAdaptor;
use crate::services::proxy::stream_event::{StreamEvent, StreamUsage, StopReason};

/// Parses OpenAI Responses API SSE format (event-typed, e.g. response.output_text.delta)
pub struct ResponsesApiAdaptor;

impl ResponseAdaptor for ResponsesApiAdaptor {
    fn parse_sse_data(&self, data: &str) -> Vec<StreamEvent> {
        let data = data.trim();
        if data.is_empty() {
            return vec![];
        }

        let Ok(json) = serde_json::from_str::<Value>(data) else {
            return vec![];
        };

        let event_type = json.get("type").and_then(|t| t.as_str()).unwrap_or("");
        self.parse_responses_event(event_type, &json)
    }

    fn parse_sse_line(&self, event_type: Option<&str>, data: &str) -> Vec<StreamEvent> {
        let data = data.trim();
        if data.is_empty() {
            return vec![];
        }

        let Ok(json) = serde_json::from_str::<Value>(data) else {
            return vec![];
        };

        let etype = event_type
            .or_else(|| json.get("type").and_then(|t| t.as_str()))
            .unwrap_or("");

        self.parse_responses_event(etype, &json)
    }

    fn parse_response(&self, body: &Value) -> Vec<StreamEvent> {
        let mut events = Vec::new();

        let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("resp").to_string();
        let model = body.get("model").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        events.push(StreamEvent::MessageStart { id, model });

        // Parse output items
        if let Some(Value::Array(output)) = body.get("output") {
            for (i, item) in output.iter().enumerate() {
                let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                let index = i as u32;

                match item_type {
                    "message" => {
                        if let Some(Value::Array(content)) = item.get("content") {
                            for part in content {
                                let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                if part_type == "output_text" {
                                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                        events.push(StreamEvent::ContentDelta { index, text: text.to_string() });
                                    }
                                }
                            }
                        }
                    }
                    "function_call" => {
                        let call_id = item.get("call_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let name = item.get("name").and_then(|n| n.as_str()).map(String::from);
                        let args = item.get("arguments").and_then(|a| a.as_str()).unwrap_or("").to_string();
                        events.push(StreamEvent::ToolCallDelta {
                            index,
                            tool_call_id: call_id,
                            name,
                            arguments: args,
                        });
                    }
                    _ => {}
                }
            }
        }

        // Usage
        if let Some(usage) = body.get("usage") {
            let u = parse_responses_usage(usage);
            events.push(StreamEvent::MessageDelta { stop_reason: StopReason::EndTurn, usage: Some(u) });
        } else {
            events.push(StreamEvent::MessageDelta { stop_reason: StopReason::EndTurn, usage: None });
        }

        events.push(StreamEvent::MessageStop);
        events
    }
}

impl ResponsesApiAdaptor {
    fn parse_responses_event(&self, event_type: &str, json: &Value) -> Vec<StreamEvent> {
        match event_type {
            "response.created" => {
                let response = json.get("response").unwrap_or(json);
                let id = response.get("id").and_then(|v| v.as_str()).unwrap_or("resp").to_string();
                let model = response.get("model").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                vec![StreamEvent::MessageStart { id, model }]
            }

            "response.output_text.delta" => {
                let text = json.get("delta").and_then(|d| d.as_str()).unwrap_or("");
                if text.is_empty() {
                    return vec![];
                }
                let index = json.get("content_index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                vec![StreamEvent::ContentDelta { index, text: text.to_string() }]
            }

            "response.output_text.done" => {
                let index = json.get("content_index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                vec![StreamEvent::ContentStop { index }]
            }

            "response.reasoning_summary_text.delta" => {
                let text = json.get("delta").and_then(|d| d.as_str()).unwrap_or("");
                if text.is_empty() {
                    return vec![];
                }
                let index = json.get("summary_index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                vec![StreamEvent::ThinkingDelta { index, text: text.to_string() }]
            }

            "response.function_call_arguments.delta" => {
                let delta = json.get("delta").and_then(|d| d.as_str()).unwrap_or("");
                let index = json.get("output_index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                let item_id = json.get("item_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                vec![StreamEvent::ToolCallDelta {
                    index,
                    tool_call_id: item_id,
                    name: None,
                    arguments: delta.to_string(),
                }]
            }

            "response.function_call_arguments.done" => {
                let index = json.get("output_index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                vec![StreamEvent::ContentStop { index }]
            }

            "response.output_item.added" => {
                // Check if it's a function_call to emit tool name
                if let Some(item) = json.get("item") {
                    let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if item_type == "function_call" {
                        let call_id = item.get("call_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let name = item.get("name").and_then(|n| n.as_str()).map(String::from);
                        let index = json.get("output_index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                        return vec![StreamEvent::ToolCallDelta {
                            index,
                            tool_call_id: call_id,
                            name,
                            arguments: String::new(),
                        }];
                    }
                }
                vec![]
            }

            "response.completed" | "response.done" => {
                let mut events = Vec::new();
                if let Some(response) = json.get("response") {
                    if let Some(usage) = response.get("usage") {
                        events.push(StreamEvent::UsageUpdate(parse_responses_usage(usage)));
                    }
                    let status = response.get("status").and_then(|s| s.as_str()).unwrap_or("completed");
                    let stop_reason = match status {
                        "completed" => StopReason::EndTurn,
                        "incomplete" => StopReason::MaxTokens,
                        "cancelled" | "canceled" => StopReason::EndTurn,
                        _ => StopReason::EndTurn,
                    };
                    let usage = response.get("usage").map(|u| parse_responses_usage(u));
                    events.push(StreamEvent::MessageDelta { stop_reason, usage });
                }
                events.push(StreamEvent::MessageStop);
                events
            }

            "response.failed" | "response.incomplete" | "response.cancelled" | "response.canceled" => {
                vec![
                    StreamEvent::MessageDelta { stop_reason: StopReason::EndTurn, usage: None },
                    StreamEvent::MessageStop,
                ]
            }

            // Events we can safely ignore
            "response.in_progress" | "response.output_item.done" |
            "response.content_part.added" | "response.content_part.done" |
            "response.reasoning_summary_part.added" | "response.reasoning_summary_part.done" |
            "response.reasoning_summary_text.done" => {
                vec![]
            }

            _ => vec![]
        }
    }
}

fn parse_responses_usage(usage: &Value) -> StreamUsage {
    StreamUsage {
        input_tokens: usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        output_tokens: usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        cache_read_tokens: usage.get("input_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        cache_creation_tokens: usage.get("input_tokens_details")
            .and_then(|d| d.get("cache_write_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
    }
}
