// OpenAI response adaptor — parses chat.completion.chunk SSE format

use serde_json::Value;
use super::ResponseAdaptor;
use crate::services::proxy::stream_event::{StreamEvent, StreamUsage, StopReason};

pub struct OpenAIResponseAdaptor;

impl ResponseAdaptor for OpenAIResponseAdaptor {
    fn parse_sse_data(&self, data: &str) -> Vec<StreamEvent> {
        let data = data.trim();

        // [DONE] signal
        if data == "[DONE]" {
            return vec![StreamEvent::MessageStop];
        }

        let Ok(json) = serde_json::from_str::<Value>(data) else {
            return vec![];
        };

        let mut events = Vec::new();

        // Extract message ID and model
        let id = json.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let model = json.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let choices = json.get("choices").and_then(|c| c.as_array());

        // Usage-only chunk (choices: [] + usage present)
        if let Some(usage) = json.get("usage") {
            let is_usage_only = choices.map_or(true, |arr| arr.is_empty());
            if is_usage_only {
                events.push(StreamEvent::UsageUpdate(parse_openai_usage(usage)));
                return events;
            }
        }

        let Some(choices) = choices else {
            return vec![StreamEvent::Ping];
        };

        if choices.is_empty() {
            return vec![StreamEvent::Ping];
        }

        for choice in choices {
            let index = choice.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
            let delta = choice.get("delta");
            let finish_reason = choice.get("finish_reason").and_then(|f| f.as_str());

            if let Some(delta) = delta {
                // Role-only delta → MessageStart
                if delta.get("role").is_some() && delta.get("content").is_none()
                    && delta.get("tool_calls").is_none()
                {
                    events.push(StreamEvent::MessageStart { id: id.clone(), model: model.clone() });
                    continue;
                }

                // Content delta
                if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                    if !content.is_empty() {
                        events.push(StreamEvent::ContentDelta { index, text: content.to_string() });
                    }
                }

                // Reasoning/thinking content (DeepSeek, OpenAI o-series)
                let reasoning = delta.get("reasoning_content")
                    .or_else(|| delta.get("reasoning"))
                    .and_then(|r| r.as_str());
                if let Some(reasoning_text) = reasoning {
                    if !reasoning_text.is_empty() {
                        events.push(StreamEvent::ThinkingDelta { index, text: reasoning_text.to_string() });
                    }
                }

                // Tool calls
                if let Some(Value::Array(tool_calls)) = delta.get("tool_calls") {
                    for tc in tool_calls {
                        let tc_index = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                        let tc_id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let func = tc.get("function");
                        let name = func.and_then(|f| f.get("name")).and_then(|n| n.as_str()).map(String::from);
                        let args = func.and_then(|f| f.get("arguments")).and_then(|a| a.as_str()).unwrap_or("").to_string();
                        events.push(StreamEvent::ToolCallDelta {
                            index: tc_index,
                            tool_call_id: tc_id,
                            name,
                            arguments: args,
                        });
                    }
                }
            }

            // Finish reason
            if let Some(reason) = finish_reason {
                let usage = json.get("usage").map(|u| parse_openai_usage(u));
                events.push(StreamEvent::MessageDelta {
                    stop_reason: StopReason::from_openai(reason),
                    usage,
                });
            }
        }

        if events.is_empty() {
            vec![StreamEvent::Ping]
        } else {
            events
        }
    }

    fn parse_response(&self, body: &Value) -> Vec<StreamEvent> {
        let mut events = Vec::new();

        let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("chatcmpl").to_string();
        let model = body.get("model").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        events.push(StreamEvent::MessageStart { id, model });

        if let Some(choices) = body.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                let index = choice.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                if let Some(msg) = choice.get("message") {
                    if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                        events.push(StreamEvent::ContentDelta { index, text: content.to_string() });
                    }
                }
                let finish_reason = choice.get("finish_reason")
                    .and_then(|f| f.as_str())
                    .unwrap_or("stop");
                let usage = body.get("usage").map(|u| parse_openai_usage(u));
                events.push(StreamEvent::MessageDelta {
                    stop_reason: StopReason::from_openai(finish_reason),
                    usage,
                });
            }
        }

        events.push(StreamEvent::MessageStop);
        events
    }
}

fn parse_openai_usage(usage: &Value) -> StreamUsage {
    StreamUsage {
        input_tokens: usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        output_tokens: usage.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        cache_read_tokens: usage.get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        cache_creation_tokens: 0,
    }
}
