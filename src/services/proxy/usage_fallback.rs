//! Usage fallback and tool call billing extraction
//!
//! When a provider doesn't return usage info in the response/stream,
//! estimates token counts from text content length.
//! Also extracts billable tool calls from SSE events.
//!
//! Tool call pricing follows new-api's surcharge model:
//! - Prices are in $/1K calls (configurable via system_settings "ToolPrice")
//! - Supports model-prefix overrides: "tool_name:model_prefix*" → override price
//! - Function calls only billed when explicitly priced by admin

use serde_json::Value;
use std::collections::HashMap;
use crate::models::{TokenUsage, BillableToolCall, BillableToolCallType};
use crate::database::Database;
use crate::database::dao::SystemSettingsDao;

/// Estimate output tokens from response text content.
/// Uses a rough heuristic: ~4 characters per token for English,
/// ~2 characters per token for CJK. We use a blend of ~3.5 chars/token.
pub fn estimate_output_tokens(text: &str) -> i64 {
    if text.is_empty() {
        return 0;
    }
    // Count characters (Unicode-aware)
    let char_count = text.chars().count() as f64;
    // Rough average: 3.5 chars per token
    (char_count / 3.5).ceil() as i64
}

/// Estimate input tokens from request body size.
/// Used as a last resort when the provider doesn't return input_tokens.
pub fn estimate_input_tokens_from_body(body_bytes: usize) -> i64 {
    // Roughly 4 bytes per token for typical request content
    (body_bytes as f64 / 4.0).ceil().max(1.0) as i64
}

/// Build a fallback TokenUsage from estimated text output and optional request body size.
pub fn build_fallback_usage(output_text: &str, request_body_bytes: Option<usize>) -> TokenUsage {
    let output_tokens = estimate_output_tokens(output_text);
    let input_tokens = request_body_bytes
        .map(|b| estimate_input_tokens_from_body(b))
        .unwrap_or(0);

    TokenUsage {
        input_tokens,
        output_tokens,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
    }
}

/// Extract accumulated text content from collected SSE events.
/// Supports multiple formats:
/// - OpenAI: choices[].delta.content
/// - Anthropic: content_block_delta.delta.text
/// - Responses API: response.output_text.delta (delta field)
/// - Gemini: candidates[].content.parts[].text
pub fn extract_text_from_events(events: &[Value]) -> String {
    let mut text = String::new();

    for event in events {
        // OpenAI Chat Completions format
        if let Some(choices) = event.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                if let Some(content) = choice
                    .get("delta")
                    .and_then(|d| d.get("content"))
                    .and_then(|c| c.as_str())
                {
                    text.push_str(content);
                }
            }
        }

        // Anthropic format
        if let Some(event_type) = event.get("type").and_then(|t| t.as_str()) {
            match event_type {
                "content_block_delta" => {
                    if let Some(delta_text) = event
                        .get("delta")
                        .and_then(|d| d.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        text.push_str(delta_text);
                    }
                }
                // Responses API format
                "response.output_text.delta" => {
                    if let Some(delta) = event.get("delta").and_then(|d| d.as_str()) {
                        text.push_str(delta);
                    }
                }
                _ => {}
            }
        }

        // Gemini format
        if let Some(candidates) = event.get("candidates").and_then(|c| c.as_array()) {
            for candidate in candidates {
                if let Some(parts) = candidate
                    .get("content")
                    .and_then(|c| c.get("parts"))
                    .and_then(|p| p.as_array())
                {
                    for part in parts {
                        if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                            text.push_str(t);
                        }
                    }
                }
            }
        }
    }

    text
}

/// Extract billable tool calls from collected SSE events.
/// Supports:
/// - Responses API: response.output_item.done with item.type = web_search_call/file_search_call/function_call
/// - OpenAI Chat: choices[].delta.tool_calls (count distinct tool call IDs)
/// - Anthropic: tool_use content blocks
pub fn extract_billable_tool_calls(events: &[Value]) -> Vec<BillableToolCall> {
    let mut web_search_count: u32 = 0;
    let mut file_search_count: u32 = 0;
    let mut function_calls: Vec<String> = Vec::new(); // function names
    let mut image_gen_count: u32 = 0;

    // Track OpenAI tool call IDs to avoid double-counting
    let mut seen_tool_call_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for event in events {
        // Responses API format: response.output_item.done
        if let Some(event_type) = event.get("type").and_then(|t| t.as_str()) {
            match event_type {
                "response.output_item.done" | "response.output_item.added" => {
                    if let Some(item) = event.get("item") {
                        let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match item_type {
                            "web_search_call" => web_search_count += 1,
                            "file_search_call" => file_search_count += 1,
                            "function_call" => {
                                let name = item.get("name")
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                function_calls.push(name);
                            }
                            "image_generation_call" => image_gen_count += 1,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        // OpenAI Chat Completions: choices[].delta.tool_calls
        if let Some(choices) = event.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                if let Some(tool_calls) = choice
                    .get("delta")
                    .and_then(|d| d.get("tool_calls"))
                    .and_then(|tc| tc.as_array())
                {
                    for tc in tool_calls {
                        if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                            if seen_tool_call_ids.insert(id.to_string()) {
                                // New tool call — count it
                                let name = tc
                                    .get("function")
                                    .and_then(|f| f.get("name"))
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                function_calls.push(name);
                            }
                        }
                    }
                }
            }
        }

        // Anthropic: content_block_start with type=tool_use
        if let Some(event_type) = event.get("type").and_then(|t| t.as_str()) {
            if event_type == "content_block_start" {
                if let Some(content_block) = event.get("content_block") {
                    let block_type = content_block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if block_type == "tool_use" {
                        let name = content_block
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        function_calls.push(name);
                    }
                }
            }
        }
    }

    let mut calls = Vec::new();

    if web_search_count > 0 {
        calls.push(BillableToolCall {
            call_type: BillableToolCallType::WebSearch,
            name: None,
            count: web_search_count,
        });
    }
    if file_search_count > 0 {
        calls.push(BillableToolCall {
            call_type: BillableToolCallType::FileSearch,
            name: None,
            count: file_search_count,
        });
    }
    if image_gen_count > 0 {
        calls.push(BillableToolCall {
            call_type: BillableToolCallType::ImageGeneration,
            name: None,
            count: image_gen_count,
        });
    }
    // Group function calls by name
    if !function_calls.is_empty() {
        let mut fn_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for name in function_calls {
            *fn_counts.entry(name).or_insert(0) += 1;
        }
        for (name, count) in fn_counts {
            calls.push(BillableToolCall {
                call_type: BillableToolCallType::FunctionCall,
                name: Some(name),
                count,
            });
        }
    }

    calls
}

/// Calculate the cost of billable tool calls.
///
/// Pricing model (aligned with new-api):
/// - Prices stored as $/1K calls in system_settings key "ToolPrice"
/// - Formula: cost = price × count / 1000 × group_ratio
/// - Supports model-prefix overrides: "tool_name:model_prefix*"
/// - Default hardcoded prices ($/1K calls):
///   - web_search: 10.0
///   - web_search_preview: 10.0 (gpt-4o/gpt-4.1: 25.0)
///   - file_search: 2.5
///   - google_search: 14.0
///   - image_generation: 150.0
/// - Function calls: $0 by default, only billed if admin configures a price
pub fn calculate_tool_call_cost(
    tool_calls: &[BillableToolCall],
    model_name: Option<&str>,
    group_ratio: f64,
    tool_prices: &ToolPriceConfig,
) -> f64 {
    let mut cost = 0.0;

    for call in tool_calls {
        let tool_name = call.tool_name_for_pricing();
        let price_per_1k = tool_prices.get_price(&tool_name, model_name);

        if price_per_1k <= 0.0 {
            continue;
        }

        // Formula: price × count / 1000 × group_ratio
        cost += price_per_1k * call.count as f64 / 1000.0 * group_ratio;
    }

    cost
}

// ---------------------------------------------------------------------------
// Tool Price Configuration
// ---------------------------------------------------------------------------

/// Default tool prices ($/1K calls) — same as new-api hardcoded defaults
pub const DEFAULT_WEB_SEARCH_PRICE: f64 = 10.0;
pub const DEFAULT_WEB_SEARCH_PREVIEW_PRICE: f64 = 10.0;
pub const DEFAULT_FILE_SEARCH_PRICE: f64 = 2.5;
pub const DEFAULT_GOOGLE_SEARCH_PRICE: f64 = 14.0;
pub const DEFAULT_IMAGE_GENERATION_PRICE: f64 = 150.0;
/// Higher price for search-preview on GPT-4o/4.1 series
pub const DEFAULT_SEARCH_PREVIEW_GPT4O_PRICE: f64 = 25.0;

/// Tool price configuration with model-prefix overrides
#[derive(Debug, Clone)]
pub struct ToolPriceConfig {
    /// Default prices: tool_name → $/1K calls
    pub defaults: HashMap<String, f64>,
    /// Model-prefix overrides: tool_name → [(prefix, price)]
    /// Sorted by prefix length descending (longest match first)
    pub prefix_overrides: HashMap<String, Vec<(String, f64)>>,
}

impl Default for ToolPriceConfig {
    fn default() -> Self {
        let mut defaults = HashMap::new();
        defaults.insert("web_search".to_string(), DEFAULT_WEB_SEARCH_PRICE);
        defaults.insert("web_search_preview".to_string(), DEFAULT_WEB_SEARCH_PREVIEW_PRICE);
        defaults.insert("file_search".to_string(), DEFAULT_FILE_SEARCH_PRICE);
        defaults.insert("google_search".to_string(), DEFAULT_GOOGLE_SEARCH_PRICE);
        defaults.insert("image_generation".to_string(), DEFAULT_IMAGE_GENERATION_PRICE);

        let mut prefix_overrides: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        // GPT-4o/4.1 series have higher web_search_preview pricing
        let gpt_overrides = vec![
            ("gpt-4o".to_string(), DEFAULT_SEARCH_PREVIEW_GPT4O_PRICE),
            ("gpt-4.1".to_string(), DEFAULT_SEARCH_PREVIEW_GPT4O_PRICE),
            ("gpt-4o-mini".to_string(), DEFAULT_SEARCH_PREVIEW_GPT4O_PRICE),
            ("gpt-4.1-mini".to_string(), DEFAULT_SEARCH_PREVIEW_GPT4O_PRICE),
        ];
        prefix_overrides.insert("web_search_preview".to_string(), gpt_overrides);

        Self { defaults, prefix_overrides }
    }
}

impl ToolPriceConfig {
    /// Load tool prices from system_settings database.
    /// Key: "ToolPrice" — JSON object with entries like:
    ///   - "web_search": 10.0
    ///   - "web_search_preview:gpt-4o*": 25.0
    /// Falls back to hardcoded defaults for missing entries.
    pub fn load(db: &Database) -> Self {
        let mut config = Self::default();

        // Load operator overrides from DB
        if let Ok(Some(setting)) = SystemSettingsDao::get(db, "ToolPrice") {
            // setting.value is a serde_json::Value (expected to be an object of { tool_name: price })
            if let Ok(prices) = serde_json::from_value::<HashMap<String, f64>>(setting.value) {
                for (key, price) in prices {
                    if price.is_nan() || price.is_infinite() {
                        continue;
                    }
                    if let Some((tool_name, model_prefix)) = key.split_once(':') {
                        // Model-prefix override: "tool_name:model_prefix*"
                        let prefix = model_prefix.trim_end_matches('*').to_string();
                        config.prefix_overrides
                            .entry(tool_name.to_string())
                            .or_default()
                            .push((prefix, price));
                    } else {
                        // Default price override
                        config.defaults.insert(key, price);
                    }
                }
            }
        }

        // Sort prefix overrides by length descending (longest match first)
        for overrides in config.prefix_overrides.values_mut() {
            overrides.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        }

        config
    }

    /// Get the effective price for a tool + model combination.
    /// Returns $/1K calls. Returns 0.0 if no price is configured.
    pub fn get_price(&self, tool_name: &str, model_name: Option<&str>) -> f64 {
        // Check model-prefix overrides first (longest prefix match)
        if let Some(model) = model_name {
            if let Some(overrides) = self.prefix_overrides.get(tool_name) {
                for (prefix, price) in overrides {
                    if model.starts_with(prefix.as_str()) {
                        return *price;
                    }
                }
            }
        }

        // Fall back to default price
        self.defaults.get(tool_name).copied().unwrap_or(0.0)
    }
}

/// Surcharge item for logging (matches new-api's ToolSurchargeItem)
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolSurchargeItem {
    pub name: String,
    pub count: u32,
    /// Price in $/1K calls
    pub price_per_1k: f64,
    /// Computed cost for this item in USD
    pub cost: f64,
}

/// Build surcharge items with computed costs for logging/auditing
pub fn build_tool_surcharge_items(
    tool_calls: &[BillableToolCall],
    model_name: Option<&str>,
    group_ratio: f64,
    tool_prices: &ToolPriceConfig,
) -> Vec<ToolSurchargeItem> {
    let mut items = Vec::new();

    for call in tool_calls {
        let tool_name = call.tool_name_for_pricing();
        let price_per_1k = tool_prices.get_price(&tool_name, model_name);

        if price_per_1k <= 0.0 {
            continue;
        }

        let cost = price_per_1k * call.count as f64 / 1000.0 * group_ratio;
        items.push(ToolSurchargeItem {
            name: tool_name,
            count: call.count,
            price_per_1k,
            cost,
        });
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_estimate_output_tokens() {
        assert_eq!(estimate_output_tokens(""), 0);
        assert_eq!(estimate_output_tokens("Hello, world!"), 4); // 13 chars / 3.5 ≈ 4
        // Longer text
        let long_text = "a".repeat(350);
        assert_eq!(estimate_output_tokens(&long_text), 100); // 350 / 3.5 = 100
    }

    #[test]
    fn test_extract_text_openai() {
        let events = vec![
            json!({"choices": [{"delta": {"content": "Hello"}}]}),
            json!({"choices": [{"delta": {"content": " world"}}]}),
        ];
        assert_eq!(extract_text_from_events(&events), "Hello world");
    }

    #[test]
    fn test_extract_text_anthropic() {
        let events = vec![
            json!({"type": "content_block_delta", "delta": {"type": "text_delta", "text": "Hello"}}),
            json!({"type": "content_block_delta", "delta": {"type": "text_delta", "text": " world"}}),
        ];
        assert_eq!(extract_text_from_events(&events), "Hello world");
    }

    #[test]
    fn test_extract_text_responses_api() {
        let events = vec![
            json!({"type": "response.output_text.delta", "delta": "Hello"}),
            json!({"type": "response.output_text.delta", "delta": " world"}),
        ];
        assert_eq!(extract_text_from_events(&events), "Hello world");
    }

    #[test]
    fn test_extract_billable_tool_calls_responses_api() {
        let events = vec![
            json!({"type": "response.output_item.done", "item": {"type": "web_search_call", "id": "ws1"}}),
            json!({"type": "response.output_item.done", "item": {"type": "function_call", "id": "fc1", "name": "get_weather"}}),
            json!({"type": "response.output_item.done", "item": {"type": "function_call", "id": "fc2", "name": "get_weather"}}),
        ];
        let calls = extract_billable_tool_calls(&events);
        assert_eq!(calls.len(), 2); // web_search + function_call(get_weather x2)
        assert!(calls.iter().any(|c| c.call_type == BillableToolCallType::WebSearch && c.count == 1));
        assert!(calls.iter().any(|c| c.call_type == BillableToolCallType::FunctionCall && c.count == 2));
    }

    #[test]
    fn test_extract_billable_tool_calls_openai_chat() {
        let events = vec![
            json!({"choices": [{"delta": {"tool_calls": [{"id": "tc1", "function": {"name": "search"}}]}}]}),
            json!({"choices": [{"delta": {"tool_calls": [{"id": "tc1", "function": {"arguments": "{}"}}]}}]}), // same ID, should not double-count
            json!({"choices": [{"delta": {"tool_calls": [{"id": "tc2", "function": {"name": "calc"}}]}}]}),
        ];
        let calls = extract_billable_tool_calls(&events);
        assert_eq!(calls.len(), 2); // search + calc
    }

    #[test]
    fn test_tool_price_config_defaults() {
        let config = ToolPriceConfig::default();
        assert_eq!(config.get_price("web_search", None), 10.0);
        assert_eq!(config.get_price("file_search", None), 2.5);
        assert_eq!(config.get_price("image_generation", None), 150.0);
        // Unknown tool → 0
        assert_eq!(config.get_price("my_custom_tool", None), 0.0);
    }

    #[test]
    fn test_tool_price_config_model_override() {
        let config = ToolPriceConfig::default();
        // web_search_preview uses default for non-GPT models
        assert_eq!(config.get_price("web_search_preview", Some("claude-3-opus")), 10.0);
        // GPT-4o series gets the higher price
        assert_eq!(config.get_price("web_search_preview", Some("gpt-4o-2024-08-06")), 25.0);
        assert_eq!(config.get_price("web_search_preview", Some("gpt-4.1-mini-2025")), 25.0);
    }

    #[test]
    fn test_calculate_tool_call_cost() {
        let config = ToolPriceConfig::default();
        let calls = vec![
            BillableToolCall {
                call_type: BillableToolCallType::WebSearch,
                name: None,
                count: 3,
            },
            BillableToolCall {
                call_type: BillableToolCallType::FunctionCall,
                name: Some("get_weather".to_string()),
                count: 2,
            },
        ];

        // web_search: 10.0 * 3 / 1000 * 1.0 = 0.03
        // get_weather: 0.0 (no price configured for custom function)
        let cost = calculate_tool_call_cost(&calls, Some("gpt-4o"), 1.0, &config);
        assert!((cost - 0.03).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_tool_call_cost_with_group_ratio() {
        let config = ToolPriceConfig::default();
        let calls = vec![
            BillableToolCall {
                call_type: BillableToolCallType::WebSearch,
                name: None,
                count: 1,
            },
        ];

        // web_search: 10.0 * 1 / 1000 * 2.0 = 0.02
        let cost = calculate_tool_call_cost(&calls, None, 2.0, &config);
        assert!((cost - 0.02).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_tool_call_cost_image_gen() {
        let config = ToolPriceConfig::default();
        let calls = vec![
            BillableToolCall {
                call_type: BillableToolCallType::ImageGeneration,
                name: None,
                count: 2,
            },
        ];

        // image_generation: 150.0 * 2 / 1000 * 1.0 = 0.30
        let cost = calculate_tool_call_cost(&calls, None, 1.0, &config);
        assert!((cost - 0.30).abs() < 1e-10);
    }
}
