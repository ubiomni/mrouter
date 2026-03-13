//! 辅助工具函数

use axum::http::HeaderMap;
use serde_json::Value;
use crate::models::{TokenUsage, ProviderType};
use super::token_parser::{UniversalParser, ClaudeParser, OpenAIParser, CodexParser, GeminiParser, TokenParser};

/// 从响应 JSON 中提取 token 使用信息
pub fn extract_token_usage(json: &Value) -> Option<TokenUsage> {
    UniversalParser::parse_response(json)
}

/// 从响应 JSON 中提取 token 使用信息（根据 provider 类型优化）
pub fn extract_token_usage_with_type(json: &Value, provider_type: &ProviderType) -> Option<TokenUsage> {
    // 根据 provider 类型选择特定的解析器
    match provider_type {
        ProviderType::Anthropic => ClaudeParser.parse_response(json),
        ProviderType::OpenAI | ProviderType::OpenRouter | ProviderType::AzureOpenAI => {
            // OpenAI 兼容的 API，也尝试 Codex 解析器
            OpenAIParser.parse_response(json)
                .or_else(|| CodexParser.parse_response(json))
        }
        ProviderType::Google | ProviderType::GoogleVertexAI => {
            GeminiParser.parse_response(json)
        }
        _ => {
            // 未知类型，使用通用解析器
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

    // 提取 SSE 事件
    let events = super::token_parser::extract_sse_events(data);

    tracing::debug!("[TokenExtract] Extracted {} SSE events", events.len());

    // 使用通用解析器
    let usage = UniversalParser::parse_stream_events(&events);

    if let Some(ref u) = usage {
        tracing::info!(
            "[TokenExtract] ✓ Extracted usage: input={}, output={}, cache_creation={}, cache_read={}, total={}",
            u.input_tokens,
            u.output_tokens,
            u.cache_creation_tokens,
            u.cache_read_tokens,
            u.total_tokens()
        );
    } else {
        tracing::warn!("[TokenExtract] ✗ No token usage found in SSE stream");
    }

    usage
}

/// 从 SSE 流数据中提取 token 使用信息（根据 provider 类型优化）
pub fn extract_token_usage_from_sse_with_type(data: &[u8], provider_type: &ProviderType) -> Option<TokenUsage> {
    tracing::debug!(
        "[TokenExtract] Starting SSE token extraction (type: {:?}), data size: {} bytes",
        provider_type,
        data.len()
    );

    // 提取 SSE 事件
    let events = super::token_parser::extract_sse_events(data);

    tracing::debug!("[TokenExtract] Extracted {} SSE events", events.len());

    // 根据 provider 类型选择特定的解析器
    let usage = match provider_type {
        ProviderType::Anthropic => ClaudeParser.parse_stream_events(&events),
        ProviderType::OpenAI | ProviderType::OpenRouter | ProviderType::AzureOpenAI => {
            // OpenAI 兼容的 API，也尝试 Codex 解析器
            OpenAIParser.parse_stream_events(&events)
                .or_else(|| CodexParser.parse_stream_events(&events))
        }
        ProviderType::Google | ProviderType::GoogleVertexAI => {
            GeminiParser.parse_stream_events(&events)
        }
        _ => {
            // 未知类型，使用通用解析器
            UniversalParser::parse_stream_events(&events)
        }
    };

    if let Some(ref u) = usage {
        tracing::info!(
            "[TokenExtract] ✓ Extracted usage: input={}, output={}, cache_creation={}, cache_read={}, total={}",
            u.input_tokens,
            u.output_tokens,
            u.cache_creation_tokens,
            u.cache_read_tokens,
            u.total_tokens()
        );
    } else {
        tracing::warn!("[TokenExtract] ✗ No token usage found in SSE stream");
    }

    usage
}

/// 从请求中提取 Session ID
///
/// 支持多种来源：
/// 1. 请求头：x-session-id, x-request-id, x-correlation-id, x-trace-id
/// 2. 请求体：metadata.session_id, metadata.user_id (Claude格式)
/// 3. 请求体：previous_response_id (Codex对话延续)
pub fn extract_session_id(headers: &HeaderMap, body: &Value) -> Option<String> {
    // 1. 从请求头提取
    let session_headers = [
        "x-session-id",
        "x-request-id",
        "x-correlation-id",
        "x-trace-id",
        "request-id",
        "session-id",
    ];

    for header_name in &session_headers {
        if let Some(value) = headers.get(*header_name) {
            if let Ok(session_id) = value.to_str() {
                if !session_id.is_empty() {
                    return Some(session_id.to_string());
                }
            }
        }
    }

    // 2. 从请求体的 metadata 提取
    if let Some(metadata) = body.get("metadata") {
        // 2.1 直接从 metadata.session_id 提取
        if let Some(session_id) = metadata.get("session_id").and_then(|v| v.as_str()) {
            if !session_id.is_empty() {
                return Some(session_id.to_string());
            }
        }

        // 2.2 从 metadata.user_id 提取 (Claude 格式: user_xxx_session_yyy)
        if let Some(user_id) = metadata.get("user_id").and_then(|v| v.as_str()) {
            if let Some(pos) = user_id.find("_session_") {
                let session_id = &user_id[pos + 9..]; // "_session_" 长度为 9
                if !session_id.is_empty() {
                    return Some(session_id.to_string());
                }
            }
        }
    }

    // 3. 从 previous_response_id 提取 (Codex 对话延续)
    if let Some(prev_id) = body.get("previous_response_id").and_then(|v| v.as_str()) {
        if !prev_id.is_empty() {
            return Some(format!("prev_{}", prev_id));
        }
    }

    None
}
