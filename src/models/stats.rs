// 使用统计数据模型

use serde::{Deserialize, Serialize};

/// Token 使用详情（用于提取和传递）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

/// Billable tool call types (aligned with OpenAI Responses API billing)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BillableToolCallType {
    WebSearch,
    FileSearch,
    FunctionCall,
    ImageGeneration,
}

/// Billing type for proxy request logs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BillingType {
    Token,   // Token-based billing (standard LLM chat/completions)
    Request, // Per-request billing (passthrough, unknown formats)
    Task,    // Per-task billing (async tasks like image generation, MJ, Suno)
}

impl std::fmt::Display for BillingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BillingType::Token => write!(f, "token"),
            BillingType::Request => write!(f, "request"),
            BillingType::Task => write!(f, "task"),
        }
    }
}

impl Default for BillingType {
    fn default() -> Self {
        BillingType::Token
    }
}

impl std::fmt::Display for BillableToolCallType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WebSearch => write!(f, "web_search_call"),
            Self::FileSearch => write!(f, "file_search_call"),
            Self::FunctionCall => write!(f, "function_call"),
            Self::ImageGeneration => write!(f, "image_generation_call"),
        }
    }
}

/// Billable tool call record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillableToolCall {
    pub call_type: BillableToolCallType,
    pub name: Option<String>,
    pub count: u32,
}

impl BillableToolCall {
    /// Get the tool name used for pricing lookup.
    /// For built-in tools, use the standard name; for function calls, use the function name.
    pub fn tool_name_for_pricing(&self) -> String {
        match self.call_type {
            BillableToolCallType::WebSearch => "web_search".to_string(),
            BillableToolCallType::FileSearch => "file_search".to_string(),
            BillableToolCallType::ImageGeneration => "image_generation".to_string(),
            BillableToolCallType::FunctionCall => {
                self.name.clone().unwrap_or_else(|| "unknown".to_string())
            }
        }
    }
}

/// Extended usage info that includes tool call billing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtendedUsage {
    pub token_usage: TokenUsage,
    pub tool_calls: Vec<BillableToolCall>,
    /// Whether the token usage was estimated (not from provider)
    pub estimated: bool,
}

/// 使用统计摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub total_requests: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cache_creation_tokens: i64,
    pub total_cache_read_tokens: i64,
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
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub tokens: i64,
    pub cost: f64,
}

/// 代理请求日志（详细的每请求跟踪）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLog {
    pub id: i64,
    pub provider_id: i64,
    pub request_time: i64,
    pub response_time: Option<i64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<i64>,       // Proxy token ID (when auth_enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_name: Option<String>,  // Proxy token name (when auth_enabled)
    // --- Phase 5 fields ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default)]
    pub retry_count: i32,
    #[serde(default)]
    pub billing_type: BillingType,  // token / request / task
    // --- Billing-type-specific fields ---
    /// Per-task billing: task type name, e.g. "image_generation", "audio_generation" (billing_type = Task)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    /// Per-task billing: number of tasks/items produced (billing_type = Task)
    #[serde(default)]
    pub task_count: i64,
}
