// Provider 数据模型

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use super::AppType;

/// 定价配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingConfig {
    /// 每百万输入 tokens 的价格
    pub input_price_per_million: f64,
    /// 每百万输出 tokens 的价格
    pub output_price_per_million: f64,
    /// 每百万缓存写入 tokens 的价格
    pub cache_write_price_per_million: f64,
    /// 每百万缓存读取 tokens 的价格
    pub cache_read_price_per_million: f64,
}

/// API 格式（上游 Provider 实际支持的 API 协议）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiFormat {
    Anthropic,      // /v1/messages (Anthropic Messages API)
    OpenAI,         // /v1/chat/completions (OpenAI Chat Completions)
    Google,         // Gemini API
}

impl std::fmt::Display for ApiFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiFormat::Anthropic => write!(f, "Anthropic"),
            ApiFormat::OpenAI => write!(f, "OpenAI"),
            ApiFormat::Google => write!(f, "Google"),
        }
    }
}

impl std::str::FromStr for ApiFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "anthropic" => Ok(ApiFormat::Anthropic),
            "openai" => Ok(ApiFormat::OpenAI),
            "google" => Ok(ApiFormat::Google),
            _ => Err(format!("Unknown API format: {}", s)),
        }
    }
}

/// API Provider 类型（模型厂商/API 端点）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    Anthropic,
    OpenAI,
    Google,
    AwsBedrock,
    AzureOpenAI,
    GoogleVertexAI,
    Mistral,
    Cohere,
    DeepSeek,
    XAI,
    Meta,
    MiniMax,
    Zhipu,
    Moonshot,
    Baichuan,
    OpenRouter,
    Together,
    Fireworks,
    Groq,
    Custom,
}

impl ProviderType {
    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderType::Anthropic => "Anthropic",
            ProviderType::OpenAI => "OpenAI",
            ProviderType::Google => "Google AI Studio",
            ProviderType::AwsBedrock => "AWS Bedrock",
            ProviderType::AzureOpenAI => "Azure OpenAI",
            ProviderType::GoogleVertexAI => "Google Vertex AI",
            ProviderType::Mistral => "Mistral AI",
            ProviderType::Cohere => "Cohere",
            ProviderType::DeepSeek => "DeepSeek",
            ProviderType::XAI => "xAI",
            ProviderType::Meta => "Meta Llama",
            ProviderType::MiniMax => "MiniMax",
            ProviderType::Zhipu => "Zhipu AI",
            ProviderType::Moonshot => "Moonshot AI",
            ProviderType::Baichuan => "Baichuan",
            ProviderType::OpenRouter => "OpenRouter",
            ProviderType::Together => "Together AI",
            ProviderType::Fireworks => "Fireworks AI",
            ProviderType::Groq => "Groq",
            ProviderType::Custom => "Custom",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderType::Anthropic => "anthropic",
            ProviderType::OpenAI => "openai",
            ProviderType::Google => "google",
            ProviderType::AwsBedrock => "aws-bedrock",
            ProviderType::AzureOpenAI => "azure-openai",
            ProviderType::GoogleVertexAI => "google-vertex-ai",
            ProviderType::Mistral => "mistral",
            ProviderType::Cohere => "cohere",
            ProviderType::DeepSeek => "deepseek",
            ProviderType::XAI => "xai",
            ProviderType::Meta => "meta",
            ProviderType::MiniMax => "minimax",
            ProviderType::Zhipu => "zhipu",
            ProviderType::Moonshot => "moonshot",
            ProviderType::Baichuan => "baichuan",
            ProviderType::OpenRouter => "openrouter",
            ProviderType::Together => "together",
            ProviderType::Fireworks => "fireworks",
            ProviderType::Groq => "groq",
            ProviderType::Custom => "custom",
        }
    }

    pub fn default_base_url(&self) -> &'static str {
        match self {
            ProviderType::Anthropic => "https://api.anthropic.com",
            ProviderType::OpenAI => "https://api.openai.com",
            ProviderType::Google => "https://generativelanguage.googleapis.com",
            ProviderType::AwsBedrock => "https://bedrock-runtime.us-east-1.amazonaws.com",
            ProviderType::AzureOpenAI => "https://{resource}.openai.azure.com",
            ProviderType::GoogleVertexAI => "https://us-central1-aiplatform.googleapis.com",
            ProviderType::Mistral => "https://api.mistral.ai",
            ProviderType::Cohere => "https://api.cohere.com",
            ProviderType::DeepSeek => "https://api.deepseek.com",
            ProviderType::XAI => "https://api.x.ai",
            ProviderType::Meta => "https://api.llama.com",
            ProviderType::MiniMax => "https://api.minimax.chat",
            ProviderType::Zhipu => "https://open.bigmodel.cn/api",
            ProviderType::Moonshot => "https://api.moonshot.cn",
            ProviderType::Baichuan => "https://api.baichuan-ai.com",
            ProviderType::OpenRouter => "https://openrouter.ai/api",
            ProviderType::Together => "https://api.together.xyz",
            ProviderType::Fireworks => "https://api.fireworks.ai",
            ProviderType::Groq => "https://api.groq.com/openai",
            ProviderType::Custom => "",
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            ProviderType::Anthropic => "claude-sonnet-4-20250514",
            ProviderType::OpenAI => "gpt-4o",
            ProviderType::Google => "gemini-2.5-pro",
            ProviderType::AwsBedrock => "anthropic.claude-sonnet-4-20250514-v1:0",
            ProviderType::AzureOpenAI => "gpt-4o",
            ProviderType::GoogleVertexAI => "claude-sonnet-4@20250514",
            ProviderType::Mistral => "mistral-large-latest",
            ProviderType::Cohere => "command-r-plus",
            ProviderType::DeepSeek => "deepseek-chat",
            ProviderType::XAI => "grok-3",
            ProviderType::Meta => "llama-4-maverick",
            ProviderType::MiniMax => "MiniMax-Text-01",
            ProviderType::Zhipu => "glm-4-plus",
            ProviderType::Moonshot => "moonshot-v1-128k",
            ProviderType::Baichuan => "Baichuan4",
            ProviderType::OpenRouter => "anthropic/claude-sonnet-4",
            ProviderType::Together => "meta-llama/Llama-4-Maverick-17B-128E-Instruct-Turbo",
            ProviderType::Fireworks => "accounts/fireworks/models/llama-v3p3-70b-instruct",
            ProviderType::Groq => "llama-3.3-70b-versatile",
            ProviderType::Custom => "",
        }
    }

    /// 获取该 Provider 类型的常用模型列表（用于默认填充）
    pub fn default_supported_models(&self) -> Vec<String> {
        match self {
            ProviderType::Anthropic => vec![
                "claude-opus-4".to_string(),
                "claude-sonnet-4".to_string(),
                "claude-haiku-4".to_string(),
                "claude-opus-4-20250514".to_string(),
                "claude-sonnet-4-20250514".to_string(),
                "claude-haiku-4-20250514".to_string(),
            ],
            ProviderType::OpenAI => vec![
                "gpt-4o".to_string(),
                "gpt-4o-mini".to_string(),
                "gpt-4-turbo".to_string(),
                "gpt-4".to_string(),
                "gpt-3.5-turbo".to_string(),
            ],
            ProviderType::Google => vec![
                "gemini-2.5-pro".to_string(),
                "gemini-2.5-flash".to_string(),
                "gemini-1.5-pro".to_string(),
                "gemini-1.5-flash".to_string(),
                "gemini-pro".to_string(),
            ],
            ProviderType::DeepSeek => vec![
                "deepseek-chat".to_string(),
                "deepseek-coder".to_string(),
            ],
            ProviderType::Mistral => vec![
                "mistral-large-latest".to_string(),
                "mistral-medium-latest".to_string(),
                "mistral-small-latest".to_string(),
            ],
            ProviderType::Cohere => vec![
                "command-r-plus".to_string(),
                "command-r".to_string(),
                "command".to_string(),
                "command-light".to_string(),
            ],
            ProviderType::XAI => vec![
                "grok-3".to_string(),
                "grok-2".to_string(),
            ],
            ProviderType::Groq => vec![
                "llama-3.3-70b-versatile".to_string(),
                "llama-3.1-70b-versatile".to_string(),
                "mixtral-8x7b-32768".to_string(),
            ],
            ProviderType::Zhipu => vec![
                "glm-4-plus".to_string(),
                "glm-4".to_string(),
                "glm-3-turbo".to_string(),
            ],
            ProviderType::Moonshot => vec![
                "moonshot-v1-128k".to_string(),
                "moonshot-v1-32k".to_string(),
                "moonshot-v1-8k".to_string(),
            ],
            // 其他 Provider 类型暂不提供默认列表
            _ => vec![],
        }
    }

    /// 获取该 Provider 类型的默认定价
    pub fn default_pricing(&self) -> PricingConfig {
        match self {
            ProviderType::Anthropic => PricingConfig {
                input_price_per_million: 3.0,
                output_price_per_million: 15.0,
                cache_write_price_per_million: 3.75,
                cache_read_price_per_million: 0.30,
            },
            ProviderType::OpenAI => PricingConfig {
                input_price_per_million: 2.5,
                output_price_per_million: 10.0,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::Google => PricingConfig {
                input_price_per_million: 1.25,
                output_price_per_million: 5.0,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::DeepSeek => PricingConfig {
                input_price_per_million: 0.14,
                output_price_per_million: 0.28,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::Groq => PricingConfig {
                input_price_per_million: 0.05,
                output_price_per_million: 0.08,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::Mistral => PricingConfig {
                input_price_per_million: 1.0,
                output_price_per_million: 3.0,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::Cohere => PricingConfig {
                input_price_per_million: 1.0,
                output_price_per_million: 2.0,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::Together => PricingConfig {
                input_price_per_million: 0.2,
                output_price_per_million: 0.2,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::Fireworks => PricingConfig {
                input_price_per_million: 0.2,
                output_price_per_million: 0.2,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::OpenRouter => PricingConfig {
                input_price_per_million: 0.5,
                output_price_per_million: 1.5,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::XAI => PricingConfig {
                input_price_per_million: 5.0,
                output_price_per_million: 15.0,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::Moonshot => PricingConfig {
                input_price_per_million: 0.8,
                output_price_per_million: 0.8,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::Zhipu => PricingConfig {
                input_price_per_million: 0.5,
                output_price_per_million: 0.5,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::MiniMax => PricingConfig {
                input_price_per_million: 1.0,
                output_price_per_million: 1.0,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::Baichuan => PricingConfig {
                input_price_per_million: 0.5,
                output_price_per_million: 0.5,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::AzureOpenAI => PricingConfig {
                input_price_per_million: 2.5,
                output_price_per_million: 10.0,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::AwsBedrock => PricingConfig {
                input_price_per_million: 3.0,
                output_price_per_million: 15.0,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            ProviderType::GoogleVertexAI => PricingConfig {
                input_price_per_million: 1.25,
                output_price_per_million: 5.0,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
            // Meta, Custom 等使用通用默认值
            _ => PricingConfig {
                input_price_per_million: 1.0,
                output_price_per_million: 2.0,
                cache_write_price_per_million: 0.0,
                cache_read_price_per_million: 0.0,
            },
        }
    }

    /// 获取该 Provider 类型的默认鉴权 header spec 字符串
    /// 供 TUI 预填充使用，避免重复 auth_header() 中的 match 逻辑
    pub fn default_auth_header_spec(&self) -> &'static str {
        match self {
            ProviderType::Anthropic => "x-api-key",
            ProviderType::Google => "x-goog-api-key",
            ProviderType::Custom => "x-api-key",
            _ => "authorization:Bearer",
        }
    }

    /// 返回该 Provider 类型默认的 API 格式
    pub fn default_api_format(&self) -> ApiFormat {
        match self {
            ProviderType::Anthropic => ApiFormat::Anthropic,
            ProviderType::Google | ProviderType::GoogleVertexAI => ApiFormat::Google,
            // OpenAI 及所有 OpenAI 兼容的 provider
            _ => ApiFormat::OpenAI,
        }
    }

    /// 检查给定的 URL 是否是任何 provider type 的默认 base URL
    pub fn is_default_base_url(url: &str) -> bool {

        if url.is_empty() || url.contains("auto-fill") {
            return true;
        }

        Self::all().iter().any(|pt| pt.default_base_url() == url)
    }

    pub fn all() -> Vec<ProviderType> {
        vec![
            ProviderType::Anthropic,
            ProviderType::OpenAI,
            ProviderType::Google,
            ProviderType::AwsBedrock,
            ProviderType::AzureOpenAI,
            ProviderType::GoogleVertexAI,
            ProviderType::Mistral,
            ProviderType::Cohere,
            ProviderType::DeepSeek,
            ProviderType::XAI,
            ProviderType::Meta,
            ProviderType::MiniMax,
            ProviderType::Zhipu,
            ProviderType::Moonshot,
            ProviderType::Baichuan,
            ProviderType::OpenRouter,
            ProviderType::Together,
            ProviderType::Fireworks,
            ProviderType::Groq,
            ProviderType::Custom,
        ]
    }
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl std::str::FromStr for ProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "anthropic" => Ok(ProviderType::Anthropic),
            "openai" => Ok(ProviderType::OpenAI),
            "google" | "google ai studio" => Ok(ProviderType::Google),
            "aws-bedrock" | "bedrock" | "aws bedrock" => Ok(ProviderType::AwsBedrock),
            "azure-openai" | "azure" | "azure openai" => Ok(ProviderType::AzureOpenAI),
            "google-vertex-ai" | "vertex" | "vertex-ai" | "google vertex ai" => Ok(ProviderType::GoogleVertexAI),
            "mistral" | "mistral ai" => Ok(ProviderType::Mistral),
            "cohere" => Ok(ProviderType::Cohere),
            "deepseek" => Ok(ProviderType::DeepSeek),
            "xai" | "x.ai" => Ok(ProviderType::XAI),
            "meta" | "meta llama" | "llama" => Ok(ProviderType::Meta),
            "minimax" => Ok(ProviderType::MiniMax),
            "zhipu" | "zhipu ai" => Ok(ProviderType::Zhipu),
            "moonshot" | "moonshot ai" => Ok(ProviderType::Moonshot),
            "baichuan" => Ok(ProviderType::Baichuan),
            "openrouter" => Ok(ProviderType::OpenRouter),
            "together" | "together ai" => Ok(ProviderType::Together),
            "fireworks" | "fireworks ai" => Ok(ProviderType::Fireworks),
            "groq" => Ok(ProviderType::Groq),
            "custom" => Ok(ProviderType::Custom),
            _ => Err(format!("Unknown provider type: {}", s)),
        }
    }
}

/// Provider 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: i64,
    pub app_type: AppType,
    pub provider_type: ProviderType,
    pub name: String,
    pub is_active: bool,
    pub api_key: String,
    pub base_url: String,
    pub model: Option<String>,
    pub config: serde_json::Value,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// 同步到哪些 CLI Tool（空数组表示仅使用 LLM Gateway Router）
    /// 可选值: ["claude", "codex", "gemini", "opencode", "openclaw"]
    #[serde(default)]
    pub sync_to_cli_tools: Vec<String>,
    /// 支持的模型列表（用于基于 model 参数的智能路由）
    /// 例如: ["gpt-4", "gpt-4-turbo", "gpt-3.5-turbo"]
    #[serde(default)]
    pub supported_models: Option<Vec<String>>,
    /// 是否启用 Token 使用统计
    #[serde(default = "default_enable_stats")]
    pub enable_stats: bool,
    /// API 格式（None = 根据 provider_type 自动推断）
    #[serde(default)]
    pub api_format: Option<ApiFormat>,
}

fn default_enable_stats() -> bool {
    false
}

impl Provider {
    pub fn new(app_type: AppType, provider_type: ProviderType, name: String, api_key: String, base_url: String) -> Self {
        let now = Utc::now();
        Self {
            id: 0,
            app_type,
            provider_type,
            name,
            is_active: false,
            api_key,
            base_url,
            model: None,
            config: serde_json::json!({}),
            priority: 0,
            created_at: now,
            updated_at: now,
            sync_to_cli_tools: Vec::new(),
            supported_models: None,
            enable_stats: false,
            api_format: None,
        }
    }

    /// 检查是否需要同步到指定的 CLI Tool
    pub fn should_sync_to(&self, cli_tool: &str) -> bool {
        self.sync_to_cli_tools.iter().any(|t| t == cli_tool)
    }

    /// 检查是否需要同步到任何 CLI Tool
    pub fn should_sync_to_any(&self) -> bool {
        !self.sync_to_cli_tools.is_empty()
    }

    /// 获取有效的 API 格式：有用户指定值用用户值，否则用 provider_type 默认值
    pub fn effective_api_format(&self) -> ApiFormat {
        self.api_format.unwrap_or_else(|| self.provider_type.default_api_format())
    }

    /// 检查是否支持指定的模型
    /// 会同时检查 supported_models 列表和模型映射
    pub fn supports_model(&self, model: &str) -> bool {
        if let Some(models) = &self.supported_models {
            // 1. 直接检查模型名是否在列表中
            if models.iter().any(|m| m == model) {
                return true;
            }

            // 2. 检查是否有映射：如果 model 是别名，检查映射后的实际模型名
            let actual_model = self.map_model_name(model);
            if actual_model != model && models.iter().any(|m| m == &actual_model) {
                return true;
            }

            // 3. 反向检查：如果 model 是实际模型名，检查是否有别名映射到它
            let mappings = self.model_mappings();
            for (alias, actual) in mappings.iter() {
                if actual == model && models.iter().any(|m| m == alias) {
                    return true;
                }
            }

            false
        } else {
            false
        }
    }

    /// 获取定价配置（从 config 或使用默认值）
    pub fn pricing(&self) -> PricingConfig {
        // 尝试从 config JSON 中获取，否则使用默认值
        if let Some(pricing) = self.config.get("pricing") {
            serde_json::from_value(pricing.clone())
                .unwrap_or_else(|_| self.provider_type.default_pricing())
        } else {
            self.provider_type.default_pricing()
        }
    }

    /// 获取模型映射（从 config 中读取）
    /// 返回 HashMap<别名, 实际模型名>
    pub fn model_mappings(&self) -> std::collections::HashMap<String, String> {
        if let Some(mappings) = self.config.get("model_mappings") {
            serde_json::from_value(mappings.clone()).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        }
    }

    /// 将别名映射为实际模型名
    /// 如果没有映射，返回原始名称
    pub fn map_model_name(&self, alias: &str) -> String {
        let mappings = self.model_mappings();
        mappings.get(alias).cloned().unwrap_or_else(|| alias.to_string())
    }

    /// 获取自定义请求 Headers（从 config 中读取）
    /// 返回 HashMap<header名, header值>
    pub fn custom_headers(&self) -> std::collections::HashMap<String, String> {
        if let Some(headers) = self.config.get("custom_headers") {
            serde_json::from_value(headers.clone()).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        }
    }

    /// 获取鉴权 header 名称和完整值
    /// 优先级：config.auth_header > provider_type 默认值
    ///
    /// config.auth_header 格式：
    ///   - "x-api-key"              → x-api-key: <api_key>
    ///   - "authorization:Bearer"   → authorization: Bearer <api_key>
    ///   - "x-goog-api-key"         → x-goog-api-key: <api_key>
    ///
    /// 返回 (header_name, header_value)
    pub fn auth_header(&self) -> (String, String) {
        let spec = if let Some(val) = self.config.get("auth_header").and_then(|v| v.as_str()) {
            val.to_string()
        } else {
            // 根据 provider_type 选择默认鉴权方式
            match self.provider_type {
                ProviderType::Anthropic => "x-api-key".to_string(),
                ProviderType::OpenAI | ProviderType::AzureOpenAI | ProviderType::Groq
                | ProviderType::Together | ProviderType::Fireworks | ProviderType::OpenRouter
                | ProviderType::Mistral | ProviderType::Cohere | ProviderType::DeepSeek
                | ProviderType::XAI | ProviderType::Meta | ProviderType::MiniMax
                | ProviderType::Moonshot | ProviderType::Baichuan
                => "authorization:Bearer".to_string(),
                ProviderType::Google => "x-goog-api-key".to_string(),
                ProviderType::GoogleVertexAI | ProviderType::AwsBedrock
                => "authorization:Bearer".to_string(),
                ProviderType::Zhipu => "authorization:Bearer".to_string(),
                ProviderType::Custom => "x-api-key".to_string(),
            }
        };

        // 解析 "header_name" 或 "header_name:prefix" 格式
        if let Some((name, prefix)) = spec.split_once(':') {
            (name.to_string(), format!("{} {}", prefix, self.api_key))
        } else {
            (spec, self.api_key.clone())
        }
    }
}

/// Provider 端点（用于故障转移）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEndpoint {
    pub id: i64,
    pub provider_id: i64,
    pub url: String,
    pub priority: i32,
    pub is_healthy: bool,
    pub last_check: Option<DateTime<Utc>>,
}

/// Provider 健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub provider_id: i64,
    pub is_healthy: bool,
    pub latency_ms: Option<u64>,
    pub success_rate: f64,
    pub last_error: Option<String>,
    pub last_check: DateTime<Utc>,
    pub consecutive_failures: i32,
}

impl ProviderHealth {
    pub fn status_icon(&self) -> &'static str {
        if self.is_healthy {
            "●"
        } else if self.consecutive_failures > 0 {
            "✗"
        } else {
            "⚠"
        }
    }

    pub fn status_text(&self) -> &'static str {
        if self.is_healthy {
            "Healthy"
        } else if self.consecutive_failures > 5 {
            "Down"
        } else {
            "Degraded"
        }
    }
}
