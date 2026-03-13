// 使用统计数据模型

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 使用统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    pub id: i64,
    pub provider_id: i64,
    pub timestamp: DateTime<Utc>,
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cost: f64,

    /// 缓存创建 tokens (Anthropic prompt caching)
    #[serde(default)]
    pub cache_creation_tokens: i64,

    /// 缓存读取 tokens (Anthropic prompt caching)
    #[serde(default)]
    pub cache_read_tokens: i64,
}

/// Token 使用详情（用于提取和传递）
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
}

impl TokenUsage {
    pub fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.cache_creation_tokens + self.cache_read_tokens
    }
}

impl UsageStats {
    pub fn new(provider_id: i64) -> Self {
        Self {
            id: 0,
            provider_id,
            timestamp: Utc::now(),
            request_count: 0,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cost: 0.0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
        }
    }
}

/// 使用统计摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub total_requests: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_tokens: i64,
    pub total_cost: f64,
    pub avg_cost_per_request: f64,
    pub by_provider: Vec<ProviderUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub provider_name: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub tokens: i64,
    pub cost: f64,
}

/// 代理请求日志（详细的每请求跟踪）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRequestLog {
    pub id: i64,
    pub provider_id: i64,
    pub request_time: DateTime<Utc>,
    pub response_time: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub first_token_ms: Option<i64>,  // 首 token 时间（TTFT）
    pub status_code: Option<i32>,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost: f64,
    pub error_message: Option<String>,
    pub request_path: Option<String>,
    pub request_method: Option<String>,
    pub session_id: Option<String>,  // Session ID for request tracking
}
