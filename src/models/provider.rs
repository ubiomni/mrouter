// Provider 数据模型 (Gateway 设计)

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Provider 状态枚举 (对齐 new-api ChannelStatus)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ProviderStatus {
    Enabled = 1,
    ManuallyDisabled = 2,
    AutoDisabled = 3,
}

impl Serialize for ProviderStatus {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i32(*self as i32)
    }
}

impl<'de> Deserialize<'de> for ProviderStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = i32::deserialize(deserializer)?;
        Ok(ProviderStatus::from_i32(value))
    }
}

impl ProviderStatus {
    /// Convert from database integer value
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => ProviderStatus::Enabled,
            2 => ProviderStatus::ManuallyDisabled,
            3 => ProviderStatus::AutoDisabled,
            _ => ProviderStatus::Enabled, // Default to enabled for unknown values
        }
    }

    /// Check if this status represents an active provider
    pub fn is_active(self) -> bool {
        self == ProviderStatus::Enabled
    }
}

impl fmt::Display for ProviderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderStatus::Enabled => write!(f, "Enabled"),
            ProviderStatus::ManuallyDisabled => write!(f, "Disabled"),
            ProviderStatus::AutoDisabled => write!(f, "Auto-Banned"),
        }
    }
}

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiFormat {
    Anthropic,       // /v1/messages (Anthropic Messages API)
    OpenAI,          // /v1/chat/completions (OpenAI Chat Completions)
    OpenAIResponses, // /v1/responses (OpenAI Responses API)
    Google,          // Gemini API
    Passthrough,     // No conversion — forward request as-is
}

impl std::fmt::Display for ApiFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiFormat::Anthropic => write!(f, "Anthropic"),
            ApiFormat::OpenAI => write!(f, "OpenAI"),
            ApiFormat::OpenAIResponses => write!(f, "OpenAIResponses"),
            ApiFormat::Google => write!(f, "Google"),
            ApiFormat::Passthrough => write!(f, "Passthrough"),
        }
    }
}

impl std::str::FromStr for ApiFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "anthropic" => Ok(ApiFormat::Anthropic),
            "openai" => Ok(ApiFormat::OpenAI),
            "openai_responses" | "openairesponses" => Ok(ApiFormat::OpenAIResponses),
            "google" => Ok(ApiFormat::Google),
            "passthrough" => Ok(ApiFormat::Passthrough),
            _ => Err(format!("Unknown API format: {}", s)),
        }
    }
}

/// Provider type is a plain String (canonical lowercase form like "openai", "anthropic", "zhipu_v4").
/// Metadata lookup lives in `provider_defaults` module. No enum needed.

/// Provider 配置 (Gateway 设计)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: i64,
    pub org_id: i64,
    pub name: String,
    pub display_name: Option<String>,
    pub preset_key: Option<String>,
    pub base_url: String,
    /// 支持多 key 换行分隔
    pub api_key: String,
    // 路由配置
    pub priority: i32,
    pub weight: i32,
    /// 逗号分隔多组
    pub provider_group: String,
    // 限流
    pub rpm_limit: i32,
    pub tpm_limit: i32,
    pub concurrent_limit: i32,
    // 认证配置
    pub auth_method: String,
    pub custom_headers: Option<serde_json::Value>,
    // 状态
    pub status: ProviderStatus,
    pub auto_ban: bool,
    /// 0 = not banned; >0 = auto-ban expires at this timestamp (ms)
    pub banned_until: i64,
    // 其他
    pub remark: Option<String>,
    /// Provider category: "llm", "search", "embedding", "image", "tts", "generic"
    pub category: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Provider {
    pub fn new(
        org_id: i64,
        name: String,
        api_key: String,
        base_url: String,
    ) -> Self {
        let now = Utc::now().timestamp_millis();
        Self {
            id: 0,
            org_id,
            name,
            display_name: None,
            preset_key: None,
            api_key,
            base_url,
            priority: 0,
            weight: 1,
            provider_group: "default".to_string(),
            rpm_limit: -1,
            tpm_limit: -1,
            concurrent_limit: -1,
            auth_method: "bearer".to_string(),
            custom_headers: None,
            status: ProviderStatus::Enabled,
            auto_ban: true,
            banned_until: 0,
            remark: None,
            category: "llm".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Return a clone with api_key masked for display
    pub fn with_masked_key(mut self) -> Self {
        let key = &self.api_key;
        self.api_key = if key.len() > 8 {
            format!("{}...{}", &key[..4], &key[key.len() - 4..])
        } else {
            "****".to_string()
        };
        self
    }

    /// 获取鉴权 header 名称和完整值
    /// auth_method 格式：
    ///   - "bearer"      -> Authorization: Bearer <api_key>
    ///   - "x-api-key"   -> x-api-key: <api_key>
    ///   - "query"       -> (需要在 URL query 中添加 api_key)
    ///   - "none"        -> 不添加认证头
    ///
    /// 返回 Option<(header_name, header_value)>
    pub fn auth_header_pair(&self) -> Option<(String, String)> {
        match self.auth_method.as_str() {
            "bearer" => Some(("Authorization".to_string(), format!("Bearer {}", self.api_key))),
            "x-api-key" => Some(("x-api-key".to_string(), self.api_key.clone())),
            "query" | "none" | "" => None,
            custom => {
                // Support custom format like "Authorization:Bearer" or "API-Key"
                if let Some((name, prefix)) = custom.split_once(':') {
                    Some((name.to_string(), format!("{} {}", prefix, self.api_key)))
                } else {
                    Some((custom.to_string(), self.api_key.clone()))
                }
            }
        }
    }

    /// 获取自定义请求 Headers
    pub fn custom_headers_map(&self) -> std::collections::HashMap<String, String> {
        self.custom_headers
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    /// 获取模型映射 (legacy - stored in remark as JSON)
    pub fn model_mappings(&self) -> std::collections::HashMap<String, String> {
        self.remark
            .as_ref()
            .and_then(|r| serde_json::from_str::<serde_json::Value>(r).ok())
            .and_then(|v| v.get("model_mappings").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    /// 获取输入模态列表 (stub - managed at model level in gateway mode)
    pub fn input_modalities(&self) -> Vec<String> {
        vec!["text".to_string()]
    }

    /// 获取 model-level 输入模态覆盖 (stub)
    pub fn model_input_modalities_map(&self) -> std::collections::HashMap<String, Vec<String>> {
        std::collections::HashMap::new()
    }

    /// 获取该 provider 所属的组列表
    pub fn groups(&self) -> Vec<&str> {
        self.provider_group.split(',').map(|s| s.trim()).collect()
    }

    /// 获取多个 api_key（换行分隔）
    pub fn api_keys(&self) -> Vec<&str> {
        self.api_key.lines().filter(|l| !l.trim().is_empty()).collect()
    }
}

/// Provider 健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub provider_id: i64,
    pub is_healthy: bool,
    pub latency_ms: Option<u64>,
    pub success_rate: f64,
    pub last_error: Option<String>,
    pub last_check: i64,
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
