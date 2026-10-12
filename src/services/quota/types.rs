//! Quota system types

use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

/// Quota rule loaded from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaRule {
    pub id: i64,
    pub name: String,
    pub quota_group: Option<String>,
    pub dimension: String,
    pub model_pattern: String,
    pub limit_type: String,
    pub limit_value: i64,
    pub window_secs: i64,
    pub window_tz: Option<String>,
    pub window_type: String,
    pub enabled: bool,
    pub priority: i64,
    pub provider_name_pattern: Option<String>,
    pub skip_owner: bool,
}

/// Result of a quota check
#[derive(Debug, Clone)]
pub struct QuotaCheckResult {
    pub allowed: bool,
    pub rule_name: String,
    pub limit: i64,
    pub used: i64,
    pub remaining: i64,
    pub reset_at: DateTime<Utc>,
    pub limit_type: String,
}

/// Context passed into quota check/record
#[derive(Debug, Clone)]
pub struct QuotaContext {
    pub user_id: i64,
    pub key_id: i64,
    pub org_id: i64,
    pub model: String,
    pub quota_group: Option<String>,
    pub provider_name: Option<String>,
    pub is_owner: bool,
}

/// Single usage entry returned by query_usage
#[derive(Debug, Clone)]
pub struct UsageEntry {
    pub rule_id: i64,
    pub rule_name: String,
    pub subject_key: String,
    pub model: String,
    pub used: i64,
    pub limit: i64,
    pub remaining: i64,
    pub limit_type: String,
    pub window_secs: i64,
}
