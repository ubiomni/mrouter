// Format Registry — pluggable format conversion via trait-based registry pattern

use std::collections::HashMap;
use serde_json::{json, Value};
use crate::models::ApiFormat;

/// Trait for format converters
pub trait FormatConverter: Send + Sync {
    /// Convert request body from source format to target format.
    /// Returns (converted_body, target_path).
    fn convert_request(&self, body: &Value, path: &str) -> anyhow::Result<(Value, String)>;
    /// Convert response body from target format back to source format
    fn convert_response(&self, body: &Value) -> anyhow::Result<Value>;
    /// Convert a single SSE event from target format back to source format.
    /// Input is raw data after "data: " prefix.
    fn convert_sse_event(&self, data: &str) -> anyhow::Result<String>;
}

/// Registry of format converters indexed by (source, target) pair
pub struct FormatRegistry {
    converters: HashMap<(ApiFormat, ApiFormat), Box<dyn FormatConverter>>,
}

impl FormatRegistry {
    pub fn new() -> Self {
        let mut registry = Self { converters: HashMap::new() };
        // Register all known converters
        registry.register(ApiFormat::Anthropic, ApiFormat::OpenAI, Box::new(AnthropicToOpenAI));
        registry.register(ApiFormat::OpenAI, ApiFormat::Anthropic, Box::new(OpenAIToAnthropic));
        registry.register(ApiFormat::OpenAI, ApiFormat::Google, Box::new(OpenAIToGoogle));
        registry.register(ApiFormat::Google, ApiFormat::OpenAI, Box::new(GoogleToOpenAI));
        registry.register(ApiFormat::Anthropic, ApiFormat::Google, Box::new(AnthropicToGoogle));
        registry.register(ApiFormat::Google, ApiFormat::Anthropic, Box::new(GoogleToAnthropic));
        registry
    }

    pub fn register(&mut self, from: ApiFormat, to: ApiFormat, converter: Box<dyn FormatConverter>) {
        self.converters.insert((from, to), converter);
    }

    pub fn get(&self, from: ApiFormat, to: ApiFormat) -> Option<&dyn FormatConverter> {
        self.converters.get(&(from, to)).map(|c| c.as_ref())
    }

    pub fn can_convert(&self, from: ApiFormat, to: ApiFormat) -> bool {
        from == to || self.converters.contains_key(&(from, to))
    }
}

// ─── Anthropic ↔ OpenAI ─────────────────────────────────────────────────────
// Delegates to existing format_converter module for full-fidelity conversion.

pub struct AnthropicToOpenAI;

impl FormatConverter for AnthropicToOpenAI {
    fn convert_request(&self, body: &Value, path: &str) -> anyhow::Result<(Value, String)> {
        let (converted, new_path) = super::format_converter::convert_request(
            ApiFormat::Anthropic, ApiFormat::OpenAI, body, path,
        );
        Ok((converted, new_path))
    }

    fn convert_response(&self, body: &Value) -> anyhow::Result<Value> {
        // Provider is OpenAI, client is Anthropic → convert OpenAI response to Anthropic
        Ok(super::format_converter::convert_response(ApiFormat::Anthropic, ApiFormat::OpenAI, body))
    }

    fn convert_sse_event(&self, data: &str) -> anyhow::Result<String> {
        Ok(super::format_converter::convert_sse_event(ApiFormat::Anthropic, ApiFormat::OpenAI, data))
    }
}

pub struct OpenAIToAnthropic;

impl FormatConverter for OpenAIToAnthropic {
    fn convert_request(&self, body: &Value, path: &str) -> anyhow::Result<(Value, String)> {
        let (converted, new_path) = super::format_converter::convert_request(
            ApiFormat::OpenAI, ApiFormat::Anthropic, body, path,
        );
        Ok((converted, new_path))
    }

    fn convert_response(&self, body: &Value) -> anyhow::Result<Value> {
        Ok(super::format_converter::convert_response(ApiFormat::OpenAI, ApiFormat::Anthropic, body))
    }

    fn convert_sse_event(&self, data: &str) -> anyhow::Result<String> {
        Ok(super::format_converter::convert_sse_event(ApiFormat::OpenAI, ApiFormat::Anthropic, data))
    }
}

// ─── OpenAI ↔ Google (Gemini) ────────────────────────────────────────────────

pub struct OpenAIToGoogle;

impl FormatConverter for OpenAIToGoogle {
    fn convert_request(&self, body: &Value, _path: &str) -> anyhow::Result<(Value, String)> {
        let model = body.get("model").and_then(|m| m.as_str()).unwrap_or("gemini-pro");
        let target_path = format!("/v1beta/models/{}:generateContent", model);

        let mut contents = Vec::new();
        let mut system_instruction: Option<Value> = None;

        if let Some(Value::Array(msgs)) = body.get("messages") {
            for msg in msgs {
                let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                let content_text = extract_text_content(msg);

                if role == "system" {
                    system_instruction = Some(json!({ "parts": [{ "text": content_text }] }));
                    continue;
                }

                let gemini_role = match role {
                    "assistant" => "model",
                    _ => "user",
                };
                contents.push(json!({
                    "role": gemini_role,
                    "parts": [{ "text": content_text }]
                }));
            }
        }

        let mut result = json!({ "contents": contents });
        if let Some(sys) = system_instruction {
            result.as_object_mut().unwrap().insert("systemInstruction".to_string(), sys);
        }

        // Map generation config
        let mut gen_config = serde_json::Map::new();
        if let Some(temp) = body.get("temperature") {
            gen_config.insert("temperature".to_string(), temp.clone());
        }
        if let Some(max) = body.get("max_tokens") {
            gen_config.insert("maxOutputTokens".to_string(), max.clone());
        }
        if let Some(top_p) = body.get("top_p") {
            gen_config.insert("topP".to_string(), top_p.clone());
        }
        if !gen_config.is_empty() {
            result.as_object_mut().unwrap().insert("generationConfig".to_string(), Value::Object(gen_config));
        }

        Ok((result, target_path))
    }

    fn convert_response(&self, body: &Value) -> anyhow::Result<Value> {
        // Google response → OpenAI response
        google_response_to_openai(body)
    }

    fn convert_sse_event(&self, data: &str) -> anyhow::Result<String> {
        // Basic passthrough for streaming — full conversion is complex
        Ok(data.to_string())
    }
}

pub struct GoogleToOpenAI;

impl FormatConverter for GoogleToOpenAI {
    fn convert_request(&self, body: &Value, _path: &str) -> anyhow::Result<(Value, String)> {
        let target_path = "/v1/chat/completions".to_string();
        let mut messages = Vec::new();

        // Convert systemInstruction
        if let Some(sys) = body.get("systemInstruction") {
            let text = sys.get("parts")
                .and_then(|p| p.as_array())
                .and_then(|a| a.first())
                .and_then(|p| p.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            if !text.is_empty() {
                messages.push(json!({"role": "system", "content": text}));
            }
        }

        // Convert contents
        if let Some(Value::Array(contents)) = body.get("contents") {
            for content in contents {
                let role = content.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                let openai_role = match role {
                    "model" => "assistant",
                    _ => "user",
                };
                let text = content.get("parts")
                    .and_then(|p| p.as_array())
                    .and_then(|a| a.first())
                    .and_then(|p| p.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                messages.push(json!({"role": openai_role, "content": text}));
            }
        }

        let mut result = json!({ "messages": messages });

        // Map generationConfig back
        if let Some(gen) = body.get("generationConfig") {
            if let Some(temp) = gen.get("temperature") {
                result.as_object_mut().unwrap().insert("temperature".to_string(), temp.clone());
            }
            if let Some(max) = gen.get("maxOutputTokens") {
                result.as_object_mut().unwrap().insert("max_tokens".to_string(), max.clone());
            }
            if let Some(top_p) = gen.get("topP") {
                result.as_object_mut().unwrap().insert("top_p".to_string(), top_p.clone());
            }
        }

        Ok((result, target_path))
    }

    fn convert_response(&self, body: &Value) -> anyhow::Result<Value> {
        // OpenAI response → Google response (rarely needed but included for symmetry)
        Ok(body.clone())
    }

    fn convert_sse_event(&self, data: &str) -> anyhow::Result<String> {
        Ok(data.to_string())
    }
}

// ─── Anthropic ↔ Google (chained via OpenAI as intermediate) ─────────────────

pub struct AnthropicToGoogle;

impl FormatConverter for AnthropicToGoogle {
    fn convert_request(&self, body: &Value, path: &str) -> anyhow::Result<(Value, String)> {
        // Anthropic → OpenAI → Google
        let a2o = AnthropicToOpenAI;
        let (openai_body, openai_path) = a2o.convert_request(body, path)?;
        let o2g = OpenAIToGoogle;
        o2g.convert_request(&openai_body, &openai_path)
    }

    fn convert_response(&self, body: &Value) -> anyhow::Result<Value> {
        // Google response → OpenAI response → Anthropic response
        let g2o = GoogleToOpenAI;
        let openai_resp = g2o.convert_response(body)?;
        // Convert the OpenAI-format response to Anthropic format
        Ok(super::format_converter::convert_response(ApiFormat::Anthropic, ApiFormat::OpenAI, &openai_resp))
    }

    fn convert_sse_event(&self, data: &str) -> anyhow::Result<String> {
        Ok(data.to_string())
    }
}

pub struct GoogleToAnthropic;

impl FormatConverter for GoogleToAnthropic {
    fn convert_request(&self, body: &Value, path: &str) -> anyhow::Result<(Value, String)> {
        // Google → OpenAI → Anthropic
        let g2o = GoogleToOpenAI;
        let (openai_body, openai_path) = g2o.convert_request(body, path)?;
        let o2a = OpenAIToAnthropic;
        o2a.convert_request(&openai_body, &openai_path)
    }

    fn convert_response(&self, body: &Value) -> anyhow::Result<Value> {
        // Anthropic response → OpenAI response → Google response
        // Provider is Anthropic, client is Google — convert Anthropic response to OpenAI then to Google
        let anthropic_to_openai_resp = super::format_converter::convert_response(ApiFormat::OpenAI, ApiFormat::Anthropic, body);
        // Then OpenAI → Google (passthrough for now)
        Ok(anthropic_to_openai_resp)
    }

    fn convert_sse_event(&self, data: &str) -> anyhow::Result<String> {
        Ok(data.to_string())
    }
}

// ─── Helper functions ────────────────────────────────────────────────────────

/// Extract text content from a message (handles both string and array content)
fn extract_text_content(msg: &Value) -> String {
    match msg.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => {
            parts.iter()
                .filter_map(|p| {
                    if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                        p.get("text").and_then(|t| t.as_str()).map(String::from)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("")
        }
        _ => String::new(),
    }
}

/// Convert a Google Gemini response to OpenAI chat completion format
fn google_response_to_openai(body: &Value) -> anyhow::Result<Value> {
    let text = body.get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
        .and_then(|a| a.first())
        .and_then(|p| p.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    let finish_reason = body.get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("finishReason"))
        .and_then(|r| r.as_str())
        .map(|r| match r {
            "STOP" => "stop",
            "MAX_TOKENS" => "length",
            "SAFETY" => "content_filter",
            _ => "stop",
        })
        .unwrap_or("stop");

    // Build usage from usageMetadata if present
    let usage = body.get("usageMetadata").map(|u| {
        json!({
            "prompt_tokens": u.get("promptTokenCount").and_then(|v| v.as_u64()).unwrap_or(0),
            "completion_tokens": u.get("candidatesTokenCount").and_then(|v| v.as_u64()).unwrap_or(0),
            "total_tokens": u.get("totalTokenCount").and_then(|v| v.as_u64()).unwrap_or(0),
        })
    }).unwrap_or(json!({"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}));

    Ok(json!({
        "id": "chatcmpl-google",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": text
            },
            "finish_reason": finish_reason
        }],
        "usage": usage
    }))
}
