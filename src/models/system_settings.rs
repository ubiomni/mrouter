// System Settings 数据模型

use chrono::Utc;
use serde::{Deserialize, Serialize};

/// 系统设置（key-value 存储）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSetting {
    pub key: String,
    pub value: serde_json::Value,
    pub description: Option<String>,
    pub updated_at: i64,
}

impl SystemSetting {
    pub fn new(key: String, value: serde_json::Value) -> Self {
        Self {
            key,
            value,
            description: None,
            updated_at: Utc::now().timestamp_millis(),
        }
    }

    /// 获取字符串值
    pub fn as_str(&self) -> Option<&str> {
        self.value.as_str()
    }

    /// 获取布尔值
    pub fn as_bool(&self) -> Option<bool> {
        self.value.as_bool()
    }

    /// 获取整数值
    pub fn as_i64(&self) -> Option<i64> {
        self.value.as_i64()
    }
}
