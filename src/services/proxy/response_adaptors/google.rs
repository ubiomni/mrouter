// Google Gemini response adaptor — parses Gemini streaming JSON

use serde_json::Value;
use super::ResponseAdaptor;
use crate::services::proxy::stream_event::{StreamEvent, StreamUsage, StopReason};

pub struct GoogleResponseAdaptor;

impl ResponseAdaptor for GoogleResponseAdaptor {
    fn parse_sse_data(&self, data: &str) -> Vec<StreamEvent> {
        let data = data.trim();
        if data.is_empty() {
            return vec![];
        }

        let Ok(json) = serde_json::from_str::<Value>(data) else {
            return vec![];
        };

        self.parse_gemini_chunk(&json)
    }

    fn parse_response(&self, body: &Value) -> Vec<StreamEvent> {
        // Non-streaming Gemini response has same structure as a streaming chunk
        self.parse_gemini_chunk(body)
    }
}

impl GoogleResponseAdaptor {
    fn parse_gemini_chunk(&self, json: &Value) -> Vec<StreamEvent> {
        let mut events = Vec::new();

        let candidates = json.get("candidates").and_then(|c| c.as_array());
        let Some(candidates) = candidates else {
            // Might be a usage-only update
            if let Some(usage_meta) = json.get("usageMetadata") {
                events.push(StreamEvent::UsageUpdate(parse_gemini_usage(usage_meta)));
            }
            return events;
        };

        for (i, candidate) in candidates.iter().enumerate() {
            let index = i as u32;

            // Extract text from parts
            if let Some(content) = candidate.get("content") {
                if let Some(Value::Array(parts)) = content.get("parts") {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                events.push(StreamEvent::ContentDelta { index, text: text.to_string() });
                            }
                        }

                        // Thought / thinking (Gemini 2.5 with thinking)
                        if let Some(thought) = part.get("thought").and_then(|t| t.as_str()) {
                            if !thought.is_empty() {
                                events.push(StreamEvent::ThinkingDelta { index, text: thought.to_string() });
                            }
                        }

                        // Function call
                        if let Some(fc) = part.get("functionCall") {
                            let name = fc.get("name").and_then(|n| n.as_str()).map(String::from);
                            let args = fc.get("args")
                                .map(|a| serde_json::to_string(a).unwrap_or_default())
                                .unwrap_or_default();
                            events.push(StreamEvent::ToolCallDelta {
                                index,
                                tool_call_id: String::new(),
                                name,
                                arguments: args,
                            });
                        }
                    }
                }
            }

            // Finish reason
            if let Some(reason) = candidate.get("finishReason").and_then(|r| r.as_str()) {
                let stop_reason = match reason {
                    "STOP" => StopReason::EndTurn,
                    "MAX_TOKENS" => StopReason::MaxTokens,
                    "SAFETY" => StopReason::ContentFilter,
                    _ => StopReason::EndTurn,
                };
                let usage = json.get("usageMetadata").map(|u| parse_gemini_usage(u));
                events.push(StreamEvent::MessageDelta { stop_reason, usage });
                events.push(StreamEvent::MessageStop);
            }
        }

        if events.is_empty() {
            vec![StreamEvent::Ping]
        } else {
            events
        }
    }
}

fn parse_gemini_usage(usage: &Value) -> StreamUsage {
    StreamUsage {
        input_tokens: usage.get("promptTokenCount").and_then(|v| v.as_u64()).unwrap_or(0),
        output_tokens: usage.get("candidatesTokenCount").and_then(|v| v.as_u64()).unwrap_or(0),
        cache_read_tokens: usage.get("cachedContentTokenCount").and_then(|v| v.as_u64()).unwrap_or(0),
        cache_creation_tokens: 0,
    }
}
