//! Centralized defaults for provider types.
//! Provider type is a plain String (e.g. "openai", "anthropic", "zhipu_v4").
//! This module provides lookup functions for default metadata.

use super::provider::{ApiFormat, PricingConfig};

/// Common provider types for TUI selection lists
pub const COMMON_TYPES: &[&str] = &[
    "anthropic", "openai", "google", "aws_bedrock", "azure",
    "vertex_ai", "mistral", "cohere", "deepseek", "xai",
    "meta", "minimax", "zhipu", "zhipu_v4", "moonshot",
    "baichuan", "openrouter", "together", "fireworks", "groq",
    "perplexity", "volcengine", "baidu_v2", "cloudflare", "custom",
];

/// Check if a base_url is a known default for any provider type
pub fn is_known_default_url(url: &str) -> bool {
    COMMON_TYPES.iter().any(|t| {
        let u = base_url(t);
        !u.is_empty() && u == url
    })
}

/// Get display name for a provider type string
pub fn display_name(provider_type: &str) -> &str {
    match provider_type {
        "anthropic" => "Anthropic",
        "openai" => "OpenAI",
        "google" => "Google",
        "aws_bedrock" | "aws-bedrock" => "AWS Bedrock",
        "azure" | "azure_openai" => "Azure OpenAI",
        "vertex_ai" | "google_vertex_ai" => "Google Vertex AI",
        "mistral" => "Mistral",
        "cohere" => "Cohere",
        "deepseek" => "DeepSeek",
        "xai" => "xAI",
        "meta" => "Meta",
        "minimax" => "MiniMax",
        "zhipu" | "zhipu_v4" | "zhipu_intl" => "Zhipu",
        "moonshot" => "Moonshot",
        "baichuan" => "Baichuan",
        "openrouter" => "OpenRouter",
        "together" => "Together AI",
        "fireworks" => "Fireworks AI",
        "groq" => "Groq",
        "perplexity" => "Perplexity",
        "cloudflare" => "Cloudflare",
        "volcengine" | "doubao" => "Volcengine",
        "baidu" | "baidu_v2" => "Baidu",
        "custom" => "Custom",
        other => other,
    }
}

/// Get default base URL for a provider type
pub fn base_url(provider_type: &str) -> &'static str {
    match provider_type {
        "anthropic" => "https://api.anthropic.com",
        "openai" => "https://api.openai.com",
        "google" => "https://generativelanguage.googleapis.com",
        "aws_bedrock" | "aws-bedrock" => "https://bedrock-runtime.us-east-1.amazonaws.com",
        "azure" | "azure_openai" => "",
        "vertex_ai" | "google_vertex_ai" => "https://us-central1-aiplatform.googleapis.com",
        "mistral" => "https://api.mistral.ai",
        "cohere" => "https://api.cohere.com",
        "deepseek" => "https://api.deepseek.com",
        "xai" => "https://api.x.ai",
        "meta" => "https://api.llama.com",
        "minimax" => "https://api.minimaxi.com",
        "zhipu" | "zhipu_v4" => "https://open.bigmodel.cn",
        "zhipu_intl" => "https://open.bigmodel.cn",
        "moonshot" => "https://api.moonshot.cn",
        "baichuan" => "https://api.baichuan-ai.com",
        "openrouter" => "https://openrouter.ai/api",
        "together" => "https://api.together.xyz",
        "fireworks" => "https://api.fireworks.ai",
        "groq" => "https://api.groq.com/openai",
        "perplexity" => "https://api.perplexity.ai",
        "volcengine" | "doubao" => "https://ark.cn-beijing.volces.com/api",
        "baidu_v2" => "https://qianfan.baidubce.com",
        "cloudflare" => "https://api.cloudflare.com",
        _ => "",
    }
}

/// Get default auth header spec for a provider type.
/// Format: "header-name" or "header-name:Prefix"
pub fn auth_header_spec(provider_type: &str) -> &'static str {
    match provider_type {
        "anthropic" => "x-api-key",
        "google" | "vertex_ai" | "google_vertex_ai" => "x-goog-api-key",
        _ => "authorization:Bearer",
    }
}

/// Get default API format for a provider type
pub fn api_format(provider_type: &str) -> ApiFormat {
    match provider_type {
        "anthropic" => ApiFormat::Anthropic,
        "google" | "vertex_ai" | "google_vertex_ai" => ApiFormat::Google,
        _ => ApiFormat::OpenAI,
    }
}

/// Get default pricing for a provider type
pub fn pricing(provider_type: &str) -> PricingConfig {
    match provider_type {
        "anthropic" => PricingConfig {
            input_price_per_million: 3.0,
            output_price_per_million: 15.0,
            cache_write_price_per_million: 3.75,
            cache_read_price_per_million: 0.30,
        },
        "openai" | "azure" | "azure_openai" => PricingConfig {
            input_price_per_million: 2.5,
            output_price_per_million: 10.0,
            cache_write_price_per_million: 0.0,
            cache_read_price_per_million: 0.0,
        },
        "google" | "vertex_ai" | "google_vertex_ai" => PricingConfig {
            input_price_per_million: 1.25,
            output_price_per_million: 5.0,
            cache_write_price_per_million: 0.0,
            cache_read_price_per_million: 0.0,
        },
        "deepseek" => PricingConfig {
            input_price_per_million: 0.14,
            output_price_per_million: 0.28,
            cache_write_price_per_million: 0.0,
            cache_read_price_per_million: 0.0,
        },
        "groq" => PricingConfig {
            input_price_per_million: 0.05,
            output_price_per_million: 0.08,
            cache_write_price_per_million: 0.0,
            cache_read_price_per_million: 0.0,
        },
        "mistral" => PricingConfig {
            input_price_per_million: 1.0,
            output_price_per_million: 3.0,
            cache_write_price_per_million: 0.0,
            cache_read_price_per_million: 0.0,
        },
        "cohere" => PricingConfig {
            input_price_per_million: 1.0,
            output_price_per_million: 2.0,
            cache_write_price_per_million: 0.0,
            cache_read_price_per_million: 0.0,
        },
        "together" => PricingConfig {
            input_price_per_million: 0.2,
            output_price_per_million: 0.2,
            cache_write_price_per_million: 0.0,
            cache_read_price_per_million: 0.0,
        },
        "fireworks" => PricingConfig {
            input_price_per_million: 0.2,
            output_price_per_million: 0.2,
            cache_write_price_per_million: 0.0,
            cache_read_price_per_million: 0.0,
        },
        "openrouter" => PricingConfig {
            input_price_per_million: 0.5,
            output_price_per_million: 1.5,
            cache_write_price_per_million: 0.0,
            cache_read_price_per_million: 0.0,
        },
        "xai" => PricingConfig {
            input_price_per_million: 5.0,
            output_price_per_million: 15.0,
            cache_write_price_per_million: 0.0,
            cache_read_price_per_million: 0.0,
        },
        "volcengine" | "doubao" => PricingConfig {
            input_price_per_million: 0.3,
            output_price_per_million: 0.6,
            cache_write_price_per_million: 0.0,
            cache_read_price_per_million: 0.0,
        },
        _ => PricingConfig {
            input_price_per_million: 1.0,
            output_price_per_million: 2.0,
            cache_write_price_per_million: 0.0,
            cache_read_price_per_million: 0.0,
        },
    }
}

/// Normalize a provider type string from various user inputs to canonical form.
/// e.g. "Azure OpenAI" → "azure", "aws-bedrock" → "aws_bedrock"
pub fn normalize(input: &str) -> String {
    match input.to_lowercase().replace('-', "_").as_str() {
        "anthropic" => "anthropic",
        "openai" => "openai",
        "google" | "gemini" => "google",
        "aws_bedrock" | "bedrock" => "aws_bedrock",
        "azure_openai" | "azure" => "azure",
        "google_vertex_ai" | "vertex" | "vertex_ai" => "vertex_ai",
        "mistral" => "mistral",
        "cohere" => "cohere",
        "deepseek" => "deepseek",
        "xai" | "x_ai" => "xai",
        "meta" | "llama" => "meta",
        "minimax" => "minimax",
        "zhipu" | "chatglm" => "zhipu",
        "zhipu_v4" => "zhipu_v4",
        "zhipu_intl" => "zhipu_intl",
        "moonshot" | "kimi" => "moonshot",
        "baichuan" => "baichuan",
        "openrouter" | "open_router" => "openrouter",
        "together" | "together_ai" => "together",
        "fireworks" | "fireworks_ai" => "fireworks",
        "groq" => "groq",
        "perplexity" => "perplexity",
        "volcengine" | "doubao" => "volcengine",
        "baidu_v2" => "baidu_v2",
        "cloudflare" => "cloudflare",
        "custom" => "custom",
        other => return other.to_string(),
    }
    .to_string()
}
