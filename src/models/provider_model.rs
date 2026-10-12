// Provider Model 数据模型

use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Provider 支持的模型（provider_models 表）
/// 驱动路由：Provider 支持什么模型、走什么映射、优先级权重
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModel {
    pub id: i64,
    pub org_id: i64,
    pub provider_id: i64,
    /// 模型匹配模式：精确名 "gpt-4o" / 前缀通配 "claude-*" / 全通配 "*"
    pub model_pattern: String,
    /// 映射到上游的实际模型名（None = 同名透传）
    pub upstream_model: Option<String>,
    /// 模型级优先级覆盖（0 = 继承 provider 默认）
    pub priority: i32,
    /// 模型级权重覆盖（0 = 继承 provider 默认）
    pub weight: i32,
    /// 是否启用
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ProviderModel {
    pub fn new(org_id: i64, provider_id: i64, model_pattern: String) -> Self {
        let now = Utc::now().timestamp_millis();
        Self {
            id: 0,
            org_id,
            provider_id,
            model_pattern,
            upstream_model: None,
            priority: 0,
            weight: 0,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// 获取发送给上游的实际模型名
    pub fn effective_model(&self) -> &str {
        self.upstream_model.as_deref().unwrap_or(&self.model_pattern)
    }
}

/// 模型展示元数据（models_metadata 表）
/// 纯可选展示层 — 管理员想给模型加 icon/description/tags 就填，不填照样路由
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub org_id: i64,
    /// 模型标识符, e.g. "claude-sonnet-4-20250514"
    pub model_name: String,
    /// 模型显示名称
    pub display_name: Option<String>,
    /// 模型所属厂商: "anthropic","openai","google"
    pub vendor: Option<String>,
    /// 分组：OpenAI / Claude / 国产 / 开源
    pub model_group: Option<String>,
    /// 模型图标 URL 或内置 icon 标识
    pub icon: Option<String>,
    /// 标签: "vision,thinking,128K"
    pub tags: Option<String>,
    /// 能力: ["vision","audio","function_calling","thinking"]
    pub capabilities: Vec<String>,
    /// 是否在列表中可见
    pub is_visible: bool,
    /// 排序序号
    pub sort_order: i32,
}

impl ModelMetadata {
    pub fn new(org_id: i64, model_name: String) -> Self {
        Self {
            org_id,
            model_name,
            display_name: None,
            vendor: None,
            model_group: None,
            icon: None,
            tags: None,
            capabilities: vec![],
            is_visible: true,
            sort_order: 0,
        }
    }
}
