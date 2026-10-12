// Organization 数据模型

use chrono::Utc;
use serde::{Deserialize, Serialize};

/// 组织
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: i64,
    pub name: String,
    pub display_name: Option<String>,
    pub is_active: bool,
    pub settings: Option<serde_json::Value>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Organization {
    pub fn new(name: String) -> Self {
        let now = Utc::now().timestamp_millis();
        Self {
            id: 0,
            name,
            display_name: None,
            is_active: true,
            settings: None,
            created_at: now,
            updated_at: now,
        }
    }
}
