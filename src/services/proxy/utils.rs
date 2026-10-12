//! 辅助工具函数

use axum::http::HeaderMap;
use serde_json::Value;
use crate::models::{TokenUsage, ApiFormat};
use super::token_parser::{UniversalParser, AnthropicParser, OpenAIParser, CodexParser, GeminiParser, TokenParser};

/// 从响应 JSON 中提取 token 使用信息
pub fn extract_token_usage(json: &Value) -> Option<TokenUsage> {
    UniversalParser::parse_response(json)
}

/// 从响应 JSON 中提取 token 使用信息（根据 provider 类型优化）
pub fn extract_token_usage_with_type(json: &Value, provider_type: &str) -> Option<TokenUsage> {
    match provider_type {
        "anthropic" => AnthropicParser.parse_response(json),
        "openai" | "openrouter" | "azure" | "azure_openai" => {
            OpenAIParser.parse_response(json)
                .or_else(|| CodexParser.parse_response(json))
        }
        "google" | "vertex_ai" | "google_vertex_ai" => {
            GeminiParser.parse_response(json)
        }
        _ => {
            UniversalParser::parse_response(json)
        }
    }
}

/// 从 SSE 流数据中提取 token 使用信息
pub fn extract_token_usage_from_sse(data: &[u8]) -> Option<TokenUsage> {
    tracing::debug!(
        "[TokenExtract] Starting SSE token extraction, data size: {} bytes",
        data.len()
    );

    let events = super::token_parser::extract_sse_events(data);
    tracing::debug!("[TokenExtract] Extracted {} SSE events", events.len());

    let usage = UniversalParser::parse_stream_events(&events);

    if let Some(ref u) = usage {
        tracing::info!(
            "[TokenExtract] ✓ Extracted usage: input={}, output={}, cache_creation={}, cache_read={}, total={}",
            u.input_tokens, u.output_tokens, u.cache_creation_tokens, u.cache_read_tokens, u.total_tokens()
        );
    } else {
        tracing::warn!("[TokenExtract] ✗ No token usage found in SSE stream");
    }

    usage
}

/// 从 SSE 流数据中提取 token 使用信息（根据 provider 类型优化）
pub fn extract_token_usage_from_sse_with_type(data: &[u8], provider_type: &str) -> Option<TokenUsage> {
    tracing::debug!(
        "[TokenExtract] Starting SSE token extraction (type: {}), data size: {} bytes",
        provider_type, data.len()
    );

    let events = super::token_parser::extract_sse_events(data);
    tracing::debug!("[TokenExtract] Extracted {} SSE events", events.len());

    let usage = match provider_type {
        "anthropic" => AnthropicParser.parse_stream_events(&events),
        "openai" | "openrouter" | "azure" | "azure_openai" => {
            OpenAIParser.parse_stream_events(&events)
                .or_else(|| CodexParser.parse_stream_events(&events))
        }
        "google" | "vertex_ai" | "google_vertex_ai" => {
            GeminiParser.parse_stream_events(&events)
        }
        _ => {
            UniversalParser::parse_stream_events(&events)
        }
    };

    if let Some(ref u) = usage {
        tracing::info!(
            "[TokenExtract] ✓ Extracted usage: input={}, output={}, cache_creation={}, cache_read={}, total={}",
            u.input_tokens, u.output_tokens, u.cache_creation_tokens, u.cache_read_tokens, u.total_tokens()
        );
    }

    usage
}

/// 根据 ApiFormat 提取 token 使用信息
pub fn extract_token_usage_with_format(json: &Value, api_format: ApiFormat) -> Option<TokenUsage> {
    match api_format {
        ApiFormat::Anthropic => AnthropicParser.parse_response(json),
        ApiFormat::OpenAI | ApiFormat::OpenAIResponses => {
            OpenAIParser.parse_response(json)
                .or_else(|| CodexParser.parse_response(json))
        }
        ApiFormat::Google => GeminiParser.parse_response(json),
        ApiFormat::Passthrough => None,
    }
}

/// 根据 ApiFormat 从 SSE 流中提取 token 使用信息
pub fn extract_token_usage_from_sse_with_format(data: &[u8], api_format: ApiFormat) -> Option<TokenUsage> {
    let events = super::token_parser::extract_sse_events(data);
    match api_format {
        ApiFormat::Anthropic => AnthropicParser.parse_stream_events(&events),
        ApiFormat::OpenAI | ApiFormat::OpenAIResponses => {
            OpenAIParser.parse_stream_events(&events)
                .or_else(|| CodexParser.parse_stream_events(&events))
        }
        ApiFormat::Google => GeminiParser.parse_stream_events(&events),
        ApiFormat::Passthrough => None,
    }
}

/// 从请求 headers 或 body 中提取 session ID
pub fn extract_session_id(headers: &HeaderMap, _body: &Value) -> Option<String> {
    headers.get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}
