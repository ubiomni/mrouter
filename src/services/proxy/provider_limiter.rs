//! Provider-level rate limiting: RPM, TPM, and concurrency.
//!
//! Uses Redis when the `redis` feature is enabled and a RedisPool is configured.
//! Falls back to in-memory counters for single-instance deployments.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Provider-level rate limiter.
///
/// Tracks per-provider RPM (requests per minute), TPM (tokens per minute),
/// and concurrent in-flight request counts.
pub struct ProviderRateLimiter {
    /// RPM: provider_id -> list of request timestamps within the current 60s window
    rpm_windows: RwLock<HashMap<i64, Vec<Instant>>>,
    /// TPM: provider_id -> (window_start, accumulated_tokens)
    tpm_windows: RwLock<HashMap<i64, (Instant, i64)>>,
    /// Concurrency: provider_id -> current in-flight count
    concurrency: RwLock<HashMap<i64, i32>>,
    /// Redis pool for distributed rate limiting
    #[cfg(feature = "redis")]
    pool: Option<Arc<crate::utils::redis::RedisPool>>,
}

impl ProviderRateLimiter {
    /// Create a new limiter (in-memory only).
    pub fn new() -> Self {
        Self {
            rpm_windows: RwLock::new(HashMap::new()),
            tpm_windows: RwLock::new(HashMap::new()),
            concurrency: RwLock::new(HashMap::new()),
            #[cfg(feature = "redis")]
            pool: None,
        }
    }

    /// Create a new limiter with Redis backend.
    #[cfg(feature = "redis")]
    pub fn with_redis(pool: Arc<crate::utils::redis::RedisPool>) -> Self {
        Self {
            rpm_windows: RwLock::new(HashMap::new()),
            tpm_windows: RwLock::new(HashMap::new()),
            concurrency: RwLock::new(HashMap::new()),
            pool: Some(pool),
        }
    }

    /// Check if the provider is within RPM limit. Increments the counter if allowed.
    /// Returns true if the request is permitted.
    pub async fn check_rpm(&self, provider_id: i64, rpm_limit: i32) -> bool {
        if rpm_limit < 0 {
            return true; // -1 = unlimited
        }

        // Try Redis first
        #[cfg(feature = "redis")]
        if let Some(ref pool) = self.pool {
            if let Ok(mut conn) = pool.get_conn().await {
                let key = format!(
                    "mrouter:prov_rpm:{}:{}",
                    provider_id,
                    chrono::Utc::now().format("%Y%m%d%H%M")
                );
                let count: i32 = redis::cmd("INCR").arg(&key).query_async(&mut conn).await.unwrap_or(0);
                if count == 1 {
                    let _: () = redis::cmd("EXPIRE").arg(&key).arg(60).query_async(&mut conn).await.unwrap_or(());
                }
                return count <= rpm_limit;
            }
        }

        // In-memory fallback
        let now = Instant::now();
        let window = std::time::Duration::from_secs(60);
        let mut windows = self.rpm_windows.write().await;
        let timestamps = windows.entry(provider_id).or_default();
        timestamps.retain(|t| now.duration_since(*t) < window);

        if timestamps.len() as i32 >= rpm_limit {
            return false;
        }
        timestamps.push(now);
        true
    }

    /// Check if the provider is within TPM limit.
    /// Returns true if the provider has remaining TPM budget.
    /// tpm_limit < 0 means unlimited.
    pub async fn check_tpm(&self, provider_id: i64, tpm_limit: i32) -> bool {
        if tpm_limit < 0 {
            return true; // -1 = unlimited
        }
        let limit = tpm_limit as i64;

        // Try Redis first
        #[cfg(feature = "redis")]
        if let Some(ref pool) = self.pool {
            if let Ok(mut conn) = pool.get_conn().await {
                let key = format!(
                    "mrouter:prov_tpm:{}:{}",
                    provider_id,
                    chrono::Utc::now().format("%Y%m%d%H%M")
                );
                let current: i64 = redis::cmd("GET").arg(&key).query_async(&mut conn).await.unwrap_or(0);
                return current < limit;
            }
        }

        // In-memory fallback
        let now = Instant::now();
        let window = std::time::Duration::from_secs(60);
        let windows = self.tpm_windows.read().await;
        if let Some((start, accumulated)) = windows.get(&provider_id) {
            if now.duration_since(*start) < window {
                return *accumulated < limit;
            }
        }
        true
    }

    /// Record tokens consumed by a provider (after response).
    pub async fn record_tokens(&self, provider_id: i64, tokens: i64) {
        // Redis: INCRBY
        #[cfg(feature = "redis")]
        if let Some(ref pool) = self.pool {
            if let Ok(mut conn) = pool.get_conn().await {
                let key = format!(
                    "mrouter:prov_tpm:{}:{}",
                    provider_id,
                    chrono::Utc::now().format("%Y%m%d%H%M")
                );
                let new_val: i64 = redis::cmd("INCRBY").arg(&key).arg(tokens).query_async(&mut conn).await.unwrap_or(0);
                if new_val == tokens {
                    // First increment — set TTL
                    let _: () = redis::cmd("EXPIRE").arg(&key).arg(60).query_async(&mut conn).await.unwrap_or(());
                }
                return;
            }
        }

        // In-memory fallback
        let now = Instant::now();
        let window = std::time::Duration::from_secs(60);
        let mut windows = self.tpm_windows.write().await;
        let entry = windows.entry(provider_id).or_insert((now, 0));
        if now.duration_since(entry.0) >= window {
            *entry = (now, tokens);
        } else {
            entry.1 += tokens;
        }
    }

    /// Check if the provider has concurrency slots available.
    /// concurrency_limit < 0 means unlimited.
    pub async fn check_concurrency(&self, provider_id: i64, concurrency_limit: i32) -> bool {
        if concurrency_limit < 0 {
            return true; // unlimited
        }

        // Redis: check current count
        #[cfg(feature = "redis")]
        if let Some(ref pool) = self.pool {
            if let Ok(mut conn) = pool.get_conn().await {
                let key = format!("mrouter:prov_conc:{}", provider_id);
                let current: i32 = redis::cmd("GET").arg(&key).query_async(&mut conn).await.unwrap_or(0);
                return current < concurrency_limit;
            }
        }

        // In-memory fallback
        let conc = self.concurrency.read().await;
        let current = conc.get(&provider_id).copied().unwrap_or(0);
        current < concurrency_limit
    }

    /// Acquire a concurrency slot (increment in-flight count).
    pub async fn acquire_concurrency(&self, provider_id: i64) {
        // Redis: INCR with safety TTL
        #[cfg(feature = "redis")]
        if let Some(ref pool) = self.pool {
            if let Ok(mut conn) = pool.get_conn().await {
                let key = format!("mrouter:prov_conc:{}", provider_id);
                let _: i32 = redis::cmd("INCR").arg(&key).query_async(&mut conn).await.unwrap_or(0);
                // Safety TTL — if the process crashes, keys expire after 5 min
                let _: () = redis::cmd("EXPIRE").arg(&key).arg(300).query_async(&mut conn).await.unwrap_or(());
                return;
            }
        }

        // In-memory fallback
        let mut conc = self.concurrency.write().await;
        *conc.entry(provider_id).or_insert(0) += 1;
    }

    /// Release a concurrency slot (decrement in-flight count).
    pub async fn release_concurrency(&self, provider_id: i64) {
        // Redis: DECR (floor at 0)
        #[cfg(feature = "redis")]
        if let Some(ref pool) = self.pool {
            if let Ok(mut conn) = pool.get_conn().await {
                let key = format!("mrouter:prov_conc:{}", provider_id);
                let val: i32 = redis::cmd("DECR").arg(&key).query_async(&mut conn).await.unwrap_or(0);
                if val < 0 {
                    // Floor at 0 — shouldn't happen normally
                    let _: () = redis::cmd("SET").arg(&key).arg(0).query_async(&mut conn).await.unwrap_or(());
                }
                return;
            }
        }

        // In-memory fallback
        let mut conc = self.concurrency.write().await;
        let entry = conc.entry(provider_id).or_insert(0);
        *entry = (*entry - 1).max(0);
    }

    /// Record TPM usage (alias for record_tokens, ignores tpm_limit param).
    /// Called by response_processor after getting the token count.
    pub async fn record_tpm(&self, provider_id: i64, tokens: i64, _tpm_limit: i32) {
        self.record_tokens(provider_id, tokens).await;
    }
}
