//! Token 解析器模块
//!
//! 支持多种 API 格式的 Token 使用量提取

use serde_json::Value;
use crate::models::TokenUsage;

/// Token 解析器 trait
pub trait TokenParser {
    /// 从非流式响应解析 token 使用量
    fn parse_response(&self, body: &Value) -> Option<TokenUsage>;

    /// 从流式响应事件解析 token 使用量
    fn parse_stream_events(&self, events: &[Value]) -> Option<TokenUsage>;
}

/// Claude API 解析器
pub struct ClaudeParser;

impl TokenParser for ClaudeParser {
    fn parse_response(&self, body: &Value) -> Option<TokenUsage> {
        let usage = body.get("usage")?;

        Some(TokenUsage {
            input_tokens: usage.get("input_tokens")?.as_i64()?,
            output_tokens: usage.get("output_tokens")?.as_i64()?,
            cache_read_tokens: usage
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            cache_creation_tokens: usage
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
        })
    }

    fn parse_stream_events(&self, events: &[Value]) -> Option<TokenUsage> {
        let mut usage = TokenUsage::default();
        let mut found_any = false;

        for event in events {
            if let Some(event_type) = event.get("type").and_then(|v| v.as_str()) {
                match event_type {
                    "message_start" => {
                        // 从 message_start 提取 input tokens 和 cache tokens
                        if let Some(message) = event.get("message") {
                            if let Some(msg_usage) = message.get("usage") {
                                if let Some(input) = msg_usage.get("input_tokens").and_then(|v| v.as_i64()) {
                                    usage.input_tokens = input;
                                    found_any = true;
                                }
                                usage.cache_read_tokens = msg_usage
                                    .get("cache_read_input_tokens")
                                    .and_then(|v| v.as_i64())
                                    .unwrap_or(0);
                                usage.cache_creation_tokens = msg_usage
                                    .get("cache_creation_input_tokens")
                                    .and_then(|v| v.as_i64())
                                    .unwrap_or(0);

                                if usage.cache_read_tokens > 0 || usage.cache_creation_tokens > 0 {
                                    found_any = true;
                                }
                            }
                        }
                    }
                    "message_delta" => {
                        // 从 message_delta 获取 output tokens
                        if let Some(delta_usage) = event.get("usage") {
                            if let Some(output) = delta_usage.get("output_tokens").and_then(|v| v.as_i64()) {
                                usage.output_tokens = output;
                                found_any = true;
                            }
                        }
                    }
                    "message_stop" => {
                        // message_stop 也可能包含最终的 usage
                        if let Some(stop_usage) = event.get("usage") {
                            if let Some(output) = stop_usage.get("output_tokens").and_then(|v| v.as_i64()) {
                                usage.output_tokens = output;
                                found_any = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if found_any {
            Some(usage)
        } else {
            None
        }
    }
}

/// OpenAI/OpenRouter API 解析器
pub struct OpenAIParser;

impl TokenParser for OpenAIParser {
    fn parse_response(&self, body: &Value) -> Option<TokenUsage> {
        let usage = body.get("usage")?;

        // OpenAI 使用 prompt_tokens 和 completion_tokens
        let prompt_tokens = usage.get("prompt_tokens")?.as_i64()?;
        let completion_tokens = usage.get("completion_tokens")?.as_i64()?;

        // 获取 cached_tokens (可能在 prompt_tokens_details 中)
        let cached_tokens = usage
            .get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        Some(TokenUsage {
            input_tokens: prompt_tokens,
            output_tokens: completion_tokens,
            cache_read_tokens: cached_tokens,
            cache_creation_tokens: 0,
        })
    }

    fn parse_stream_events(&self, events: &[Value]) -> Option<TokenUsage> {
        let mut usage = TokenUsage::default();
        let mut found_any = false;

        // OpenAI 在开启 stream_options: {"include_usage": true} 时，
        // 会在最后返回一个包含 usage 对象的 chunk
        for event in events {
            if let Some(usage_obj) = event.get("usage") {
                if !usage_obj.is_null() {
                    if let Some(input) = usage_obj.get("prompt_tokens").and_then(|v| v.as_i64()) {
                        usage.input_tokens = input;
                        found_any = true;
                    }
                    if let Some(output) = usage_obj.get("completion_tokens").and_then(|v| v.as_i64()) {
                        usage.output_tokens = output;
                        found_any = true;
                    }
                    // 获取缓存 tokens
                    if let Some(details) = usage_obj.get("prompt_tokens_details") {
                        usage.cache_read_tokens = details
                            .get("cached_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                    }
                }
            }
        }

        if found_any {
            Some(usage)
        } else {
            None
        }
    }
}

/// Codex API 解析器
pub struct CodexParser;

impl TokenParser for CodexParser {
    fn parse_response(&self, body: &Value) -> Option<TokenUsage> {
        let usage = body.get("usage")?;

        // Codex 支持两种格式：
        // 1. /v1/responses: 使用 input_tokens/output_tokens
        // 2. /v1/chat/completions: 使用 prompt_tokens/completion_tokens (OpenAI 格式)

        // 检测格式
        if usage.get("prompt_tokens").is_some() {
            // OpenAI 格式
            return OpenAIParser.parse_response(body);
        }

        // Codex 格式
        let input_tokens = usage.get("input_tokens")?.as_i64()?;
        let output_tokens = usage.get("output_tokens")?.as_i64()?;

        // 获取 cached_tokens (可能在多个位置)
        let cached_tokens = usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_i64())
            .or_else(|| {
                usage
                    .get("input_tokens_details")
                    .and_then(|d| d.get("cached_tokens"))
                    .and_then(|v| v.as_i64())
            })
            .unwrap_or(0);

        Some(TokenUsage {
            input_tokens,
            output_tokens,
            cache_read_tokens: cached_tokens,
            cache_creation_tokens: usage
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
        })
    }

    fn parse_stream_events(&self, events: &[Value]) -> Option<TokenUsage> {
        // 先尝试 Codex Responses API 格式 (response.completed 事件)
        for event in events {
            if let Some(event_type) = event.get("type").and_then(|v| v.as_str()) {
                if event_type == "response.completed" {
                    if let Some(response) = event.get("response") {
                        return Self.parse_response(response);
                    }
                }
            }
        }

        // 回退到 OpenAI Chat Completions 格式
        OpenAIParser.parse_stream_events(events)
    }
}

/// Gemini API 解析器
pub struct GeminiParser;

impl TokenParser for GeminiParser {
    fn parse_response(&self, body: &Value) -> Option<TokenUsage> {
        let usage = body.get("usageMetadata")?;

        let prompt_tokens = usage.get("promptTokenCount")?.as_i64()?;
        let total_tokens = usage.get("totalTokenCount")?.as_i64()?;

        // 输出 tokens = 总 tokens - 输入 tokens
        let output_tokens = total_tokens.saturating_sub(prompt_tokens);

        Some(TokenUsage {
            input_tokens: prompt_tokens,
            output_tokens,
            cache_read_tokens: usage
                .get("cachedContentTokenCount")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            cache_creation_tokens: 0,
        })
    }

    fn parse_stream_events(&self, events: &[Value]) -> Option<TokenUsage> {
        let mut total_input = 0i64;
        let mut total_tokens = 0i64;
        let mut total_cache_read = 0i64;

        for event in events {
            if let Some(usage) = event.get("usageMetadata") {
                // 输入 tokens (通常在所有 chunk 中保持不变)
                total_input = usage
                    .get("promptTokenCount")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                // 总 tokens (包含输入 + 输出)
                total_tokens = usage
                    .get("totalTokenCount")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                // 缓存读取 tokens
                total_cache_read = usage
                    .get("cachedContentTokenCount")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
            }
        }

        // 输出 tokens = 总 tokens - 输入 tokens
        let total_output = total_tokens.saturating_sub(total_input);

        if total_input > 0 || total_output > 0 {
            Some(TokenUsage {
                input_tokens: total_input,
                output_tokens: total_output,
                cache_read_tokens: total_cache_read,
                cache_creation_tokens: 0,
            })
        } else {
            None
        }
    }
}

/// 通用解析器（尝试多种格式）
pub struct UniversalParser;

impl UniversalParser {
    /// 尝试使用所有已知的解析器
    pub fn parse_response(body: &Value) -> Option<TokenUsage> {
        // 尝试 Claude 格式
        if let Some(usage) = ClaudeParser.parse_response(body) {
            tracing::debug!("[TokenParser] Parsed as Claude format");
            return Some(usage);
        }

        // 尝试 Gemini 格式
        if let Some(usage) = GeminiParser.parse_response(body) {
            tracing::debug!("[TokenParser] Parsed as Gemini format");
            return Some(usage);
        }

        // 尝试 Codex 格式（包含 OpenAI 格式）
        if let Some(usage) = CodexParser.parse_response(body) {
            tracing::debug!("[TokenParser] Parsed as Codex/OpenAI format");
            return Some(usage);
        }

        tracing::debug!("[TokenParser] No known format matched");
        None
    }

    /// 尝试使用所有已知的流式解析器
    pub fn parse_stream_events(events: &[Value]) -> Option<TokenUsage> {
        // 尝试 Claude 格式
        if let Some(usage) = ClaudeParser.parse_stream_events(events) {
            tracing::debug!("[TokenParser] Parsed stream as Claude format");
            return Some(usage);
        }

        // 尝试 Gemini 格式
        if let Some(usage) = GeminiParser.parse_stream_events(events) {
            tracing::debug!("[TokenParser] Parsed stream as Gemini format");
            return Some(usage);
        }

        // 尝试 Codex 格式（包含 OpenAI 格式）
        if let Some(usage) = CodexParser.parse_stream_events(events) {
            tracing::debug!("[TokenParser] Parsed stream as Codex/OpenAI format");
            return Some(usage);
        }

        tracing::debug!("[TokenParser] No known stream format matched");
        None
    }
}

/// 从 SSE 原始数据中提取事件列表
pub fn extract_sse_events(data: &[u8]) -> Vec<Value> {
    let text = String::from_utf8_lossy(data);
    let mut events = Vec::new();

    for line in text.lines() {
        if let Some(json_str) = line.strip_prefix("data: ") {
            let json_str = json_str.trim();

            // 忽略 [DONE] 标记
            if json_str == "[DONE]" || json_str.is_empty() {
                continue;
            }

            // 尝试解析 JSON
            if let Ok(json_value) = serde_json::from_str::<Value>(json_str) {
                events.push(json_value);
            }
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_claude_response_parser() {
        let body = json!({
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "cache_read_input_tokens": 20,
                "cache_creation_input_tokens": 10
            }
        });

        let usage = ClaudeParser.parse_response(&body).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_tokens, 20);
        assert_eq!(usage.cache_creation_tokens, 10);
    }

    #[test]
    fn test_openai_response_parser() {
        let body = json!({
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "prompt_tokens_details": {
                    "cached_tokens": 20
                }
            }
        });

        let usage = OpenAIParser.parse_response(&body).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_tokens, 20);
    }

    #[test]
    fn test_codex_response_parser() {
        let body = json!({
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "cache_read_input_tokens": 20,
                "cache_creation_input_tokens": 10
            }
        });

        let usage = CodexParser.parse_response(&body).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_tokens, 20);
        assert_eq!(usage.cache_creation_tokens, 10);
    }

    #[test]
    fn test_codex_openai_format() {
        // Codex 也支持 OpenAI 格式
        let body = json!({
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50
            }
        });

        let usage = CodexParser.parse_response(&body).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
    }

    #[test]
    fn test_gemini_response_parser() {
        let body = json!({
            "usageMetadata": {
                "promptTokenCount": 100,
                "totalTokenCount": 150,
                "cachedContentTokenCount": 20
            }
        });

        let usage = GeminiParser.parse_response(&body).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_tokens, 20);
    }

    #[test]
    fn test_claude_stream_parser() {
        let events = vec![
            json!({
                "type": "message_start",
                "message": {
                    "usage": {
                        "input_tokens": 100,
                        "cache_read_input_tokens": 20
                    }
                }
            }),
            json!({
                "type": "message_delta",
                "usage": {
                    "output_tokens": 50
                }
            }),
        ];

        let usage = ClaudeParser.parse_stream_events(&events).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_tokens, 20);
    }

    #[test]
    fn test_codex_stream_parser() {
        let events = vec![
            json!({
                "type": "response.completed",
                "response": {
                    "usage": {
                        "input_tokens": 100,
                        "output_tokens": 50
                    }
                }
            }),
        ];

        let usage = CodexParser.parse_stream_events(&events).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
    }

    #[test]
    fn test_gemini_stream_parser() {
        let events = vec![
            json!({
                "usageMetadata": {
                    "promptTokenCount": 100,
                    "totalTokenCount": 120
                }
            }),
            json!({
                "usageMetadata": {
                    "promptTokenCount": 100,
                    "totalTokenCount": 150
                }
            }),
        ];

        let usage = GeminiParser.parse_stream_events(&events).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
    }
}
