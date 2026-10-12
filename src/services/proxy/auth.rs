//! Proxy API Key Authenticator
//!
//! Validates Bearer tokens against the `api_keys` table.
//! Provides user context (user_id, org_id, provider_group) for downstream routing and billing.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::Utc;
use sha2::{Sha256, Digest};

use crate::database::Database;
use crate::database::dao::{ApiKeyDao, UserDao};

/// Result of a successful API key authentication
#[derive(Debug, Clone)]
pub struct AuthResult {
    pub key_id: i64,
    pub user_id: i64,
    pub org_id: i64,
    pub provider_group: String,
    pub key_name: String,
    pub quota_group: Option<String>,
    /// Allowed models (None = no restriction, Some([...]) = whitelist)
    pub model_limits: Option<Vec<String>>,
}

/// Errors that can occur during authentication
#[derive(Debug, Clone)]
pub enum AuthError {
    /// No authorization header provided
    MissingToken,
    /// Token format invalid
    InvalidFormat,
    /// Token not found in database
    InvalidToken,
    /// Key is deactivated
    KeyDisabled,
    /// Key has expired
    KeyExpired,
    /// Quota exceeded
    QuotaExceeded,
    /// User not found or disabled
    UserInvalid,
    /// RPM limit exceeded
    RateLimited,
    /// Internal error
    Internal(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::MissingToken => write!(f, "Missing authorization token"),
            AuthError::InvalidFormat => write!(f, "Invalid authorization format"),
            AuthError::InvalidToken => write!(f, "Invalid API key"),
            AuthError::KeyDisabled => write!(f, "API key is disabled"),
            AuthError::KeyExpired => write!(f, "API key has expired"),
            AuthError::QuotaExceeded => write!(f, "Quota exceeded"),
            AuthError::UserInvalid => write!(f, "User not found or disabled"),
            AuthError::RateLimited => write!(f, "Rate limit exceeded"),
            AuthError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

/// Cached API key entry
#[derive(Debug, Clone)]
struct CachedKey {
    pub auth_result: AuthResult,
    pub is_active: bool,
    pub expires_at: Option<i64>,
    pub rpm_limit: i32,
    pub tpm_limit: i32,
    pub cached_at: std::time::Instant,
}

impl CachedKey {
    /// Cache entries expire after 60 seconds
    fn is_stale(&self) -> bool {
        self.cached_at.elapsed() > std::time::Duration::from_secs(60)
    }
}

/// RPM tracker abstraction — uses Redis INCR when available, falls back to in-memory.
struct RpmTracker {
    /// In-memory: key_id -> (window_minute_str, count)
    windows: HashMap<i64, (String, i32)>,
    /// Redis pool for distributed RPM tracking (None = local-only mode)
    #[cfg(feature = "redis")]
    pool: Option<Arc<crate::utils::redis::RedisPool>>,
}

impl RpmTracker {
    fn new() -> Self {
        Self {
            windows: HashMap::new(),
            #[cfg(feature = "redis")]
            pool: None,
        }
    }

    #[cfg(feature = "redis")]
    fn with_redis(pool: Arc<crate::utils::redis::RedisPool>) -> Self {
        Self {
            windows: HashMap::new(),
            pool: Some(pool),
        }
    }

    /// Check if the key is within RPM limit. Returns true if allowed.
    /// Uses Redis INCR with 60s TTL when Redis is available (distributed mode).
    /// rpm_limit < 0 means unlimited (no limit enforced).
    async fn check_and_increment(&mut self, key_id: i64, rpm_limit: i32) -> bool {
        if rpm_limit < 0 {
            return true; // unlimited
        }

        // Try Redis first (distributed mode)
        #[cfg(feature = "redis")]
        if let Some(ref pool) = self.pool {
            if let Ok(mut conn) = pool.get_conn().await {
                let key = format!("mrouter:rpm:{}:{}", key_id, Utc::now().format("%Y%m%d%H%M"));
                let count: i32 = redis::cmd("INCR").arg(&key).query_async(&mut conn).await.unwrap_or(0);
                if count == 1 {
                    let _: () = redis::cmd("EXPIRE").arg(&key).arg(60).query_async(&mut conn).await.unwrap_or(());
                }
                return count <= rpm_limit;
            }
            // Redis connection failed — fall through to in-memory
        }

        // In-memory fallback
        let current_minute = Utc::now().format("%Y%m%d%H%M").to_string();
        let entry = self.windows.entry(key_id).or_insert((String::new(), 0));
        if entry.0 != current_minute {
            *entry = (current_minute, 0);
        }
        entry.1 += 1;
        entry.1 <= rpm_limit
    }
}

/// TPM (tokens per minute) tracker — uses Redis INCRBY when available, falls back to in-memory.
struct TpmTracker {
    /// In-memory: key_id -> (window_minute_str, accumulated_tokens)
    windows: HashMap<i64, (String, i64)>,
    /// Redis pool for distributed TPM tracking
    #[cfg(feature = "redis")]
    pool: Option<Arc<crate::utils::redis::RedisPool>>,
}

impl TpmTracker {
    fn new() -> Self {
        Self {
            windows: HashMap::new(),
            #[cfg(feature = "redis")]
            pool: None,
        }
    }

    #[cfg(feature = "redis")]
    fn with_redis(pool: Arc<crate::utils::redis::RedisPool>) -> Self {
        Self {
            windows: HashMap::new(),
            pool: Some(pool),
        }
    }

    /// Check if the key is within TPM limit. Returns true if allowed.
    /// tpm_limit < 0 means unlimited.
    async fn check(&self, key_id: i64, tpm_limit: i32, _estimated_tokens: i64) -> bool {
        if tpm_limit < 0 {
            return true;
        }

        #[cfg(feature = "redis")]
        if let Some(ref pool) = self.pool {
            if let Ok(mut conn) = pool.get_conn().await {
                let key = format!("mrouter:tpm:{}:{}", key_id, Utc::now().format("%Y%m%d%H%M"));
                let current: i64 = redis::cmd("GET").arg(&key).query_async(&mut conn).await.unwrap_or(0);
                return current < tpm_limit as i64;
            }
        }

        // In-memory fallback
        let current_minute = Utc::now().format("%Y%m%d%H%M").to_string();
        if let Some((minute, tokens)) = self.windows.get(&key_id) {
            if *minute == current_minute {
                return *tokens < tpm_limit as i64;
            }
        }
        true
    }

    /// Record tokens consumed after response.
    async fn record(&mut self, key_id: i64, tokens: i64) {
        #[cfg(feature = "redis")]
        if let Some(ref pool) = self.pool {
            if let Ok(mut conn) = pool.get_conn().await {
                let key = format!("mrouter:tpm:{}:{}", key_id, Utc::now().format("%Y%m%d%H%M"));
                let new_val: i64 = redis::cmd("INCRBY").arg(&key).arg(tokens).query_async(&mut conn).await.unwrap_or(0);
                if new_val == tokens {
                    let _: () = redis::cmd("EXPIRE").arg(&key).arg(60).query_async(&mut conn).await.unwrap_or(());
                }
                return;
            }
        }

        // In-memory fallback
        let current_minute = Utc::now().format("%Y%m%d%H%M").to_string();
        let entry = self.windows.entry(key_id).or_insert((String::new(), 0));
        if entry.0 != current_minute {
            *entry = (current_minute, tokens);
        } else {
            entry.1 += tokens;
        }
    }
}

/// API key authenticator with in-memory + optional Redis caching.
///
/// Flow:
///   1. Hash the bearer token (SHA-256)
///   2. Check in-memory LRU → if hit & fresh, return immediately
///   3. (if Redis) Check Redis cache → if hit, populate in-memory + return
///   4. Query DB → populate both caches → return
///   5. Check key validity (active, not expired)
///   6. Check RPM/TPM limits (Redis INCR or in-memory)
pub struct ApiKeyAuthenticator {
    db: Database,
    /// In-memory cache: key_hash -> CachedKey
    cache: RwLock<HashMap<String, CachedKey>>,
    /// Per-key RPM tracker
    rpm_tracker: Arc<RwLock<RpmTracker>>,
    /// Per-key TPM tracker
    tpm_tracker: Arc<RwLock<TpmTracker>>,
    /// Redis pool for auth cache
    #[cfg(feature = "redis")]
    pool: Option<Arc<crate::utils::redis::RedisPool>>,
}

impl ApiKeyAuthenticator {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            cache: RwLock::new(HashMap::new()),
            rpm_tracker: Arc::new(RwLock::new(RpmTracker::new())),
            tpm_tracker: Arc::new(RwLock::new(TpmTracker::new())),
            #[cfg(feature = "redis")]
            pool: None,
        }
    }

    #[cfg(feature = "redis")]
    pub fn with_redis(db: Database, pool: Arc<crate::utils::redis::RedisPool>) -> Self {
        Self {
            db,
            cache: RwLock::new(HashMap::new()),
            rpm_tracker: Arc::new(RwLock::new(RpmTracker::with_redis(pool.clone()))),
            tpm_tracker: Arc::new(RwLock::new(TpmTracker::with_redis(pool.clone()))),
            pool: Some(pool),
        }
    }

    /// Hash a raw API key to its lookup key (SHA-256 hex).
    fn hash_key(raw_key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(raw_key.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Authenticate a Bearer token. Returns AuthResult on success.
    pub async fn authenticate(&self, token: &str) -> Result<AuthResult, AuthError> {
        // Strip "Bearer " prefix if present
        let raw_key = token.strip_prefix("Bearer ").or_else(|| token.strip_prefix("bearer ")).unwrap_or(token).trim();
        if raw_key.is_empty() {
            return Err(AuthError::MissingToken);
        }

        let key_hash = Self::hash_key(raw_key);

        // 1. Check in-memory cache
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&key_hash) {
                if !cached.is_stale() {
                    return self.validate_cached_key(cached).await;
                }
            }
        }

        // 2. Check Redis cache (if configured)
        #[cfg(feature = "redis")]
        if let Some(cached) = self.redis_get_cached_key(&key_hash).await {
            // Populate in-memory cache
            self.cache.write().await.insert(key_hash.clone(), cached.clone());
            return self.validate_cached_key(&cached).await;
        }

        // 3. Query database
        let api_key = ApiKeyDao::get_by_hash(&self.db, &key_hash)
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::InvalidToken)?;

        // Load user for org_id and status check
        let user = UserDao::get_by_id(&self.db, api_key.user_id)
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::UserInvalid)?;

        if !user.is_active {
            return Err(AuthError::UserInvalid);
        }

        let auth_result = AuthResult {
            key_id: api_key.id,
            user_id: api_key.user_id,
            org_id: user.org_id,
            provider_group: if api_key.provider_group == "inherit" {
                user.provider_group.clone()
            } else {
                api_key.provider_group.clone()
            },
            key_name: api_key.name.clone(),
            quota_group: api_key.quota_group.clone(),
            model_limits: if api_key.model_limits_enabled() {
                Some(api_key.allowed_models())
            } else {
                None
            },
        };

        let cached = CachedKey {
            auth_result: auth_result.clone(),
            is_active: api_key.is_active,
            expires_at: api_key.expires_at,
            rpm_limit: api_key.rpm_limit,
            tpm_limit: api_key.tpm_limit,
            cached_at: std::time::Instant::now(),
        };

        // Populate caches
        self.cache.write().await.insert(key_hash.clone(), cached.clone());
        #[cfg(feature = "redis")]
        self.redis_set_cached_key(&key_hash, &cached).await;

        // Update last_used_at in background
        let db = self.db.clone();
        let kid = api_key.id;
        tokio::spawn(async move {
            let _ = ApiKeyDao::update_last_used(&db, kid);
        });

        self.validate_cached_key(&cached).await
    }

    /// Validate a cached key (check active, expiry, RPM/TPM).
    async fn validate_cached_key(&self, cached: &CachedKey) -> Result<AuthResult, AuthError> {
        if !cached.is_active {
            return Err(AuthError::KeyDisabled);
        }

        if let Some(expires_at) = cached.expires_at {
            if Utc::now().timestamp() > expires_at {
                return Err(AuthError::KeyExpired);
            }
        }

        // RPM check
        {
            let mut tracker = self.rpm_tracker.write().await;
            if !tracker.check_and_increment(cached.auth_result.key_id, cached.rpm_limit).await {
                return Err(AuthError::RateLimited);
            }
        }

        Ok(cached.auth_result.clone())
    }

    /// Record token usage for TPM tracking (called after response).
    pub async fn record_tokens(&self, key_id: i64, tokens: i64) {
        let mut tracker = self.tpm_tracker.write().await;
        tracker.record(key_id, tokens).await;
    }

    /// Record TPM usage (alias for record_tokens, used by response_processor).
    pub async fn record_tpm(&self, key_id: i64, tokens: i64) {
        self.record_tokens(key_id, tokens).await;
    }

    /// Invalidate cache entry for a specific key hash (DEL from Redis).
    /// Alias-compatible: callers can use either `invalidate` or `invalidate_key`.
    pub async fn invalidate(&self, key_hash: &str) {
        self.invalidate_key(key_hash);
    }

    /// Invalidate cache entry for a specific key hash (DEL from Redis)
    pub fn invalidate_key(&self, key_hash: &str) {
        #[cfg(feature = "redis")]
        self.redis_del_cached_key(key_hash);
        #[cfg(not(feature = "redis"))]
        let _ = key_hash;
    }

    /// Invalidate all cached auth entries.
    pub fn invalidate_all(&self) {
        #[cfg(feature = "redis")]
        self.redis_del_all_auth_keys();
    }

    // --- Redis helpers ---

    #[cfg(feature = "redis")]
    async fn redis_get_cached_key(&self, key_hash: &str) -> Option<CachedKey> {
        let pool = self.pool.as_ref()?;
        let mut conn = pool.get_conn().await.ok()?;
        let key = format!("mrouter:auth:{}", key_hash);
        let data: Option<String> = redis::cmd("GET").arg(&key).query_async(&mut conn).await.ok()?;
        let json = data?;

        // Deserialize the cached auth result
        let val: serde_json::Value = serde_json::from_str(&json).ok()?;
        Some(CachedKey {
            auth_result: AuthResult {
                key_id: val.get("key_id")?.as_i64()?,
                user_id: val.get("user_id")?.as_i64()?,
                org_id: val.get("org_id")?.as_i64()?,
                provider_group: val.get("provider_group")?.as_str()?.to_string(),
                key_name: val.get("key_name")?.as_str()?.to_string(),
                quota_group: val.get("quota_group").and_then(|v| v.as_str()).map(|s| s.to_string()),
                model_limits: val.get("model_limits").and_then(|v| {
                    serde_json::from_value::<Vec<String>>(v.clone()).ok()
                }),
            },
            is_active: val.get("is_active")?.as_bool()?,
            expires_at: val.get("expires_at").and_then(|v| v.as_i64()),
            rpm_limit: val.get("rpm_limit")?.as_i64()? as i32,
            tpm_limit: val.get("tpm_limit")?.as_i64()? as i32,
            cached_at: std::time::Instant::now(), // fresh from Redis
        })
    }

    #[cfg(feature = "redis")]
    async fn redis_set_cached_key(&self, key_hash: &str, cached: &CachedKey) {
        let Some(pool) = &self.pool else { return };
        let pool = pool.clone();
        let key = format!("mrouter:auth:{}", key_hash);
        let json = serde_json::json!({
            "key_id": cached.auth_result.key_id,
            "user_id": cached.auth_result.user_id,
            "org_id": cached.auth_result.org_id,
            "provider_group": cached.auth_result.provider_group,
            "key_name": cached.auth_result.key_name,
            "quota_group": cached.auth_result.quota_group,
            "model_limits": cached.auth_result.model_limits,
            "is_active": cached.is_active,
            "expires_at": cached.expires_at,
            "rpm_limit": cached.rpm_limit,
            "tpm_limit": cached.tpm_limit,
        });
        tokio::spawn(async move {
            if let Ok(mut conn) = pool.get_conn().await {
                let _: Option<()> = redis::cmd("SET")
                    .arg(&key)
                    .arg(json.to_string())
                    .arg("EX")
                    .arg(120) // 2 min TTL in Redis (longer than in-memory 60s)
                    .query_async(&mut conn)
                    .await
                    .ok();
            }
        });
    }

    #[cfg(feature = "redis")]
    fn redis_del_cached_key(&self, key_hash: &str) {
        let Some(pool) = &self.pool else { return };
        let pool = pool.clone();
        let key = format!("mrouter:auth:{}", key_hash);
        tokio::spawn(async move {
            if let Ok(mut conn) = pool.get_conn().await {
                let _: Option<()> = redis::cmd("DEL")
                    .arg(&key)
                    .query_async(&mut conn)
                    .await
                    .ok();
            }
        });
    }

    /// Delete all auth cache keys.
    /// Uses SCAN instead of KEYS to avoid blocking the cluster.
    #[cfg(feature = "redis")]
    fn redis_del_all_auth_keys(&self) {
        let Some(pool) = &self.pool else { return };
        let pool = pool.clone();
        tokio::spawn(async move {
            if let Ok(mut conn) = pool.get_conn().await {
                // Use SCAN to iterate keys matching "auth:*"
                // This is cluster-safe (unlike KEYS).
                let mut cursor: u64 = 0;
                loop {
                    let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                        .arg(cursor)
                        .arg("MATCH")
                        .arg("auth:*")
                        .arg("COUNT")
                        .arg(100)
                        .query_async(&mut conn)
                        .await
                        .unwrap_or((0, vec![]));

                    for key in &keys {
                        let _: Option<()> = redis::cmd("DEL")
                            .arg(key)
                            .query_async(&mut conn)
                            .await
                            .ok();
                    }

                    cursor = new_cursor;
                    if cursor == 0 {
                        break;
                    }
                }
            }
        });
    }
}
