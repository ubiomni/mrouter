// User 数据模型

use chrono::Utc;
use serde::{Deserialize, Serialize};

/// 用户角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserRole {
    SuperAdmin,
    OrgAdmin,
    User,
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserRole::SuperAdmin => write!(f, "super_admin"),
            UserRole::OrgAdmin => write!(f, "org_admin"),
            UserRole::User => write!(f, "user"),
        }
    }
}

impl std::str::FromStr for UserRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "super_admin" | "superadmin" => Ok(UserRole::SuperAdmin),
            "org_admin" | "orgadmin" => Ok(UserRole::OrgAdmin),
            "user" => Ok(UserRole::User),
            _ => Err(format!("Unknown user role: {}", s)),
        }
    }
}

/// 用户
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub org_id: i64,
    pub username: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub role: UserRole,
    pub provider_group: String,
    pub is_active: bool,
    pub last_login_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl User {
    pub fn new(org_id: i64, username: String, password_hash: String) -> Self {
        let now = Utc::now().timestamp_millis();
        Self {
            id: 0,
            org_id,
            username,
            password_hash,
            display_name: None,
            email: None,
            role: UserRole::User,
            provider_group: "default".to_string(),
            is_active: true,
            last_login_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}
