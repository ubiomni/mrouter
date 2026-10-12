//! Distributed state coordination via Redis.
//!
//! Provides shared circuit breaker counters, auto-ban notifications,
//! and global request counting across multiple gateway instances.
//! When Redis is not configured (SQLite single-instance mode), all methods
//! are no-ops returning None.

use std::sync::Arc;
use super::cache_invalidation::{CacheInvalidator, InvalidationEvent};

/// Distributed state manager.
///
/// Wraps Redis interactions for multi-instance coordination.
/// All methods gracefully degrade to no-op when Redis is unavailable.
pub struct DistributedState {
    #[cfg(feature = "redis")]
    pool: Option<Arc<crate::utils::redis::RedisPool>>,
    invalidator: Arc<CacheInvalidator>,
    /// Circuit breaker failure threshold (from config)
    failure_threshold: u32,
}

impl DistributedState {
    #[cfg(feature = "redis")]
    pub fn new(
        pool: Option<Arc<crate::utils::redis::RedisPool>>,
        invalidator: Arc<CacheInvalidator>,
        failure_threshold: u32,
    ) -> Self {
        Self {
            pool,
            invalidator,
            failure_threshold,
        }
    }

    #[cfg(not(feature = "redis"))]
    pub fn new(
        invalidator: Arc<CacheInvalidator>,
        failure_threshold: u32,
    ) -> Self {
        Self {
            invalidator,
            failure_threshold,
        }
    }

    /// Increment failure count for a provider in Redis.
    /// Returns the new count, or None if Redis is unavailable.
    /// Key auto-expires after 120s (failure window).
    pub async fn incr_failure(&self, provider_id: i64) -> Option<u32> {
        #[cfg(feature = "redis")]
        {
            let pool = self.pool.as_ref()?;
            let mut conn = pool.get_conn().await.ok()?;
            let key = format!("mrouter:cb:{}:failures", provider_id);
            let count: u32 = redis::cmd("INCR")
                .arg(&key)
                .query_async(&mut conn)
                .await
                .ok()?;
            if count == 1 {
                let _: Option<()> = redis::cmd("EXPIRE")
                    .arg(&key)
                    .arg(120)
                    .query_async(&mut conn)
                    .await
                    .ok();
            }
            Some(count)
        }
        #[cfg(not(feature = "redis"))]
        {
            let _ = provider_id;
            None
        }
    }

    /// Reset failure count for a provider (called on success).
    pub async fn reset_failures(&self, provider_id: i64) {
        #[cfg(feature = "redis")]
        {
            if let Some(pool) = &self.pool {
                if let Ok(mut conn) = pool.get_conn().await {
                    let key = format!("mrouter:cb:{}:failures", provider_id);
                    let _: Option<()> = redis::cmd("DEL")
                        .arg(&key)
                        .query_async(&mut conn)
                        .await
                        .ok();
                }
            }
        }
        #[cfg(not(feature = "redis"))]
        { let _ = provider_id; }
    }

    /// Record a success for circuit breaker recovery tracking.
    /// Returns the consecutive success count, or None if Redis unavailable.
    pub async fn incr_success(&self, provider_id: i64) -> Option<u32> {
        #[cfg(feature = "redis")]
        {
            let pool = self.pool.as_ref()?;
            let mut conn = pool.get_conn().await.ok()?;
            let key = format!("mrouter:cb:{}:successes", provider_id);
            let count: u32 = redis::cmd("INCR")
                .arg(&key)
                .query_async(&mut conn)
                .await
                .ok()?;
            if count == 1 {
                let _: Option<()> = redis::cmd("EXPIRE")
                    .arg(&key)
                    .arg(300)
                    .query_async(&mut conn)
                    .await
                    .ok();
            }
            Some(count)
        }
        #[cfg(not(feature = "redis"))]
        {
            let _ = provider_id;
            None
        }
    }

    /// Reset success counter (called on failure).
    pub async fn reset_successes(&self, provider_id: i64) {
        #[cfg(feature = "redis")]
        {
            if let Some(pool) = &self.pool {
                if let Ok(mut conn) = pool.get_conn().await {
                    let key = format!("mrouter:cb:{}:successes", provider_id);
                    let _: Option<()> = redis::cmd("DEL")
                        .arg(&key)
                        .query_async(&mut conn)
                        .await
                        .ok();
                }
            }
        }
        #[cfg(not(feature = "redis"))]
        { let _ = provider_id; }
    }

    /// Publish a provider-banned event to all instances.
    pub async fn publish_ban(&self, provider_id: i64) {
        self.invalidator.publish(InvalidationEvent::ProviderBanned { provider_id }).await;
    }

    /// Alias for publish_ban (used by forwarder).
    pub async fn publish_provider_banned(&self, provider_id: i64) {
        self.publish_ban(provider_id).await;
    }

    /// Publish a provider-enabled event to all instances.
    pub async fn publish_enable(&self, provider_id: i64) {
        self.invalidator.publish(InvalidationEvent::ProviderEnabled { provider_id }).await;
    }

    /// Alias for publish_enable (used by forwarder).
    pub async fn publish_provider_enabled(&self, provider_id: i64) {
        self.publish_enable(provider_id).await;
    }

    /// Publish circuit breaker tripped event.
    pub async fn publish_circuit_breaker_tripped(&self, provider_id: i64) {
        self.invalidator.publish(InvalidationEvent::CircuitBreakerTripped { provider_id }).await;
    }

    /// Publish circuit breaker recovered event.
    pub async fn publish_circuit_breaker_recovered(&self, provider_id: i64) {
        self.invalidator.publish(InvalidationEvent::CircuitBreakerRecovered { provider_id }).await;
    }

    /// Check failure count and trip circuit breaker if threshold exceeded.
    /// Called after each provider failure in the forwarder.
    pub async fn check_and_trip(&self, provider_id: i64) {
        if let Some(count) = self.incr_failure(provider_id).await {
            if count >= self.failure_threshold {
                self.publish_circuit_breaker_tripped(provider_id).await;
            }
        }
    }

    /// Notify that a circuit breaker has recovered (provider responding again).
    /// Resets failure/success counters and publishes recovery event.
    pub async fn notify_cb_recovered(&self, provider_id: i64) {
        self.reset_failures(provider_id).await;
        self.reset_successes(provider_id).await;
        self.publish_circuit_breaker_recovered(provider_id).await;
    }

    /// Get the failure threshold (for circuit breaker logic).
    pub fn failure_threshold(&self) -> u32 {
        self.failure_threshold
    }

    /// Increment global request counter for today.
    /// Returns new count, or None if Redis unavailable.
    pub async fn incr_request_count(&self) -> Option<u64> {
        #[cfg(feature = "redis")]
        {
            let pool = self.pool.as_ref()?;
            let mut conn = pool.get_conn().await.ok()?;
            let today = chrono::Utc::now().format("%Y%m%d").to_string();
            let key = format!("mrouter:gateway:requests:{}", today);
            let count: u64 = redis::cmd("INCR")
                .arg(&key)
                .query_async(&mut conn)
                .await
                .ok()?;
            if count == 1 {
                // Expire after 48h (buffer for timezone differences)
                let _: Option<()> = redis::cmd("EXPIRE")
                    .arg(&key)
                    .arg(172800)
                    .query_async(&mut conn)
                    .await
                    .ok();
            }
            Some(count)
        }
        #[cfg(not(feature = "redis"))]
        {
            None
        }
    }

    /// Get global request count for today.
    pub async fn get_request_count(&self) -> Option<u64> {
        #[cfg(feature = "redis")]
        {
            let pool = self.pool.as_ref()?;
            let mut conn = pool.get_conn().await.ok()?;
            let today = chrono::Utc::now().format("%Y%m%d").to_string();
            let key = format!("mrouter:gateway:requests:{}", today);
            redis::cmd("GET")
                .arg(&key)
                .query_async(&mut conn)
                .await
                .ok()
        }
        #[cfg(not(feature = "redis"))]
        {
            None
        }
    }

    /// Check if distributed mode is active (Redis connected).
    #[allow(dead_code)]
    pub fn is_distributed(&self) -> bool {
        #[cfg(feature = "redis")]
        { self.pool.is_some() }
        #[cfg(not(feature = "redis"))]
        { false }
    }
}
