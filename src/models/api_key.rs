// API Key 数据模型

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

/// API Key（用户级别的访问凭证）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub key_suffix: String,
    /// Encrypted full key (can be decrypted for display)
    pub encrypted_key: Option<String>,
    /// "inherit" = 继承用户的 provider_group
    pub provider_group: String,
    /// -1 = unlimited (legacy, use quota_group for rule-based quotas)
    pub quota_limit: i64,
    /// Quota group name; when set, quota_rules engine is used instead of simple quota_limit
    pub quota_group: Option<String>,
    pub used_tokens: i64,
    pub rpm_limit: i32,
    pub tpm_limit: i32,
    /// JSON array of allowed models, e.g. ["gpt-4o", "claude-sonnet-4-20250514"]. NULL = no restriction.
    pub model_limits: Option<String>,
    pub is_active: bool,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ApiKey {
    pub fn new(user_id: i64, name: String, key_hash: String, key_prefix: String, key_suffix: String) -> Self {
        let now = Utc::now().timestamp_millis();
        Self {
            id: 0,
            user_id,
            name,
            key_hash,
            key_prefix,
            key_suffix,
            encrypted_key: None,
            provider_group: "inherit".to_string(),
            quota_limit: -1,
            quota_group: None,
            used_tokens: 0,
            rpm_limit: -1,
            tpm_limit: -1,
            model_limits: None,
            is_active: true,
            expires_at: None,
            last_used_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Generate a new API key. Returns (raw_key, ApiKey with hash+prefix+suffix+encrypted).
    pub fn generate(user_id: i64, name: &str, provider_group: &str) -> (String, Self) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let random_bytes: Vec<u8> = (0..24).map(|_| rng.gen::<u8>()).collect();
        let hex_str = hex::encode(&random_bytes); // 48 hex chars
        let raw_key = format!("sk-{}", hex_str);

        let key_hash = Self::hash_key(&raw_key);
        let key_prefix = raw_key[..8].to_string();
        let key_suffix = raw_key[raw_key.len()-4..].to_string();

        let mut key = Self::new(user_id, name.to_string(), key_hash, key_prefix, key_suffix);
        key.provider_group = provider_group.to_string();
        // Store encrypted full key for later retrieval
        key.encrypted_key = crate::crypto::encrypt(&raw_key).ok();

        (raw_key, key)
    }

    /// Hash a raw key using SHA-256
    pub fn hash_key(raw_key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(raw_key.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// 是否已过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now().timestamp_millis() > expires_at
        } else {
            false
        }
    }

    /// 是否超出配额
    pub fn is_over_quota(&self) -> bool {
        if self.quota_limit < 0 {
            false // unlimited
        } else {
            self.used_tokens >= self.quota_limit
        }
    }

    /// Whether model limits are enabled on this key
    pub fn model_limits_enabled(&self) -> bool {
        self.model_limits.as_ref().map_or(false, |s| !s.is_empty() && s != "null" && s != "[]")
    }

    /// Get the list of allowed models
    pub fn allowed_models(&self) -> Vec<String> {
        self.model_limits.as_ref()
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .unwrap_or_default()
    }

    /// Check if a model is allowed by this key (returns true if no limits set)
    pub fn is_model_allowed(&self, model: &str) -> bool {
        if !self.model_limits_enabled() {
            return true;
        }
        self.allowed_models().iter().any(|m| m == model)
    }

    /// 获取有效的 provider_group（处理 inherit 逻辑由调用方完成）
    pub fn effective_provider_group<'a>(&'a self, user_group: &'a str) -> &'a str {
        if self.provider_group == "inherit" {
            user_group
        } else {
            &self.provider_group
        }
    }
}
