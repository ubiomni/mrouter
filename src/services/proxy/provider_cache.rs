//! Provider Redis cache layer.
//!
//! When `redis` feature is enabled and Redis is configured, provider reads
//! go through Redis first. On miss, falls back to DB and backfills Redis.
//! When Redis is unavailable or the feature is disabled, all reads go directly to DB.

use anyhow::Result;
use std::sync::Arc;
use crate::database::Database;
use crate::database::dao::ProviderDao;
use crate::models::Provider;

/// TTL for individual provider cache entries (1 hour safety net)
const PROVIDER_TTL_SECS: u64 = 3600;
/// TTL for active providers list (10 minutes)
const ACTIVE_LIST_TTL_SECS: u64 = 600;

/// Provider cache backed by Redis (when available) with DB fallback.
#[derive(Clone)]
pub struct ProviderCache {
    db: Database,
    #[cfg(feature = "redis")]
    pool: Option<Arc<crate::utils::redis::RedisPool>>,
}

impl ProviderCache {
    #[cfg(feature = "redis")]
    pub fn new(db: Database, pool: Option<Arc<crate::utils::redis::RedisPool>>) -> Self {
        Self { db, pool }
    }

    #[cfg(not(feature = "redis"))]
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Get provider by ID. Redis first, DB fallback.
    pub async fn get_by_id(&self, id: i64) -> Result<Option<Provider>> {
        #[cfg(feature = "redis")]
        if let Some(provider) = self.redis_get(&format!("mrouter:provider:{}", id)).await {
            return Ok(Some(provider));
        }

        let result = ProviderDao::get_by_id(&self.db, id)?;

        #[cfg(feature = "redis")]
        if let Some(ref p) = result {
            self.redis_set(&format!("mrouter:provider:{}", id), p, PROVIDER_TTL_SECS).await;
            self.redis_set(&format!("mrouter:provider:name:{}", p.name), p, PROVIDER_TTL_SECS).await;
        }

        Ok(result)
    }

    /// Get provider by name. Redis first, DB fallback.
    pub async fn get_by_name(&self, name: &str) -> Result<Option<Provider>> {
        #[cfg(feature = "redis")]
        if let Some(provider) = self.redis_get(&format!("mrouter:provider:name:{}", name)).await {
            return Ok(Some(provider));
        }

        let result = ProviderDao::get_by_name(&self.db, name)?;

        #[cfg(feature = "redis")]
        if let Some(ref p) = result {
            self.redis_set(&format!("mrouter:provider:{}", p.id), p, PROVIDER_TTL_SECS).await;
            self.redis_set(&format!("mrouter:provider:name:{}", p.name), p, PROVIDER_TTL_SECS).await;
        }

        Ok(result)
    }

    /// Get all active providers for an org. Redis first, DB fallback.
    pub async fn get_all_active(&self, org_id: i64) -> Result<Vec<Provider>> {
        #[cfg(feature = "redis")]
        {
            let key = format!("mrouter:providers:active:{}", org_id);
            if let Some(providers) = self.redis_get_list(&key).await {
                return Ok(providers);
            }

            let result = ProviderDao::get_all_active(&self.db, org_id)?;
            self.redis_set_list(&key, &result, ACTIVE_LIST_TTL_SECS).await;
            return Ok(result);
        }

        #[cfg(not(feature = "redis"))]
        {
            ProviderDao::get_all_active(&self.db, org_id)
        }
    }

    /// Invalidate cache for a specific provider (call after create/update/delete).
    pub async fn invalidate_provider(&self, id: i64, name: &str, org_id: i64) {
        #[cfg(feature = "redis")]
        {
            self.redis_del(&format!("mrouter:provider:{}", id)).await;
            self.redis_del(&format!("mrouter:provider:name:{}", name)).await;
            self.redis_del(&format!("mrouter:providers:active:{}", org_id)).await;
        }
        #[cfg(not(feature = "redis"))]
        {
            let _ = (id, name, org_id);
        }
    }

    /// Invalidate the active providers list for an org.
    pub async fn invalidate_all_active(&self, org_id: i64) {
        #[cfg(feature = "redis")]
        self.redis_del(&format!("mrouter:providers:active:{}", org_id)).await;
        #[cfg(not(feature = "redis"))]
        let _ = org_id;
    }

    // --- Redis helpers ---

    #[cfg(feature = "redis")]
    async fn redis_get(&self, key: &str) -> Option<Provider> {
        let pool = self.pool.as_ref()?;
        let mut conn = pool.get_conn().await.ok()?;
        let data: Option<String> = redis::cmd("GET")
            .arg(key)
            .query_async(&mut conn)
            .await
            .ok()?;
        data.and_then(|s| serde_json::from_str(&s).ok())
    }

    #[cfg(feature = "redis")]
    async fn redis_get_list(&self, key: &str) -> Option<Vec<Provider>> {
        let pool = self.pool.as_ref()?;
        let mut conn = pool.get_conn().await.ok()?;
        let data: Option<String> = redis::cmd("GET")
            .arg(key)
            .query_async(&mut conn)
            .await
            .ok()?;
        data.and_then(|s| serde_json::from_str(&s).ok())
    }

    #[cfg(feature = "redis")]
    async fn redis_set(&self, key: &str, provider: &Provider, ttl: u64) {
        if let Some(pool) = &self.pool {
            if let Ok(mut conn) = pool.get_conn().await {
                if let Ok(json) = serde_json::to_string(provider) {
                    let _: Option<()> = redis::cmd("SET")
                        .arg(key)
                        .arg(&json)
                        .arg("EX")
                        .arg(ttl)
                        .query_async(&mut conn)
                        .await
                        .ok();
                }
            }
        }
    }

    #[cfg(feature = "redis")]
    async fn redis_set_list(&self, key: &str, providers: &[Provider], ttl: u64) {
        if let Some(pool) = &self.pool {
            if let Ok(mut conn) = pool.get_conn().await {
                if let Ok(json) = serde_json::to_string(providers) {
                    let _: Option<()> = redis::cmd("SET")
                        .arg(key)
                        .arg(&json)
                        .arg("EX")
                        .arg(ttl)
                        .query_async(&mut conn)
                        .await
                        .ok();
                }
            }
        }
    }

    #[cfg(feature = "redis")]
    async fn redis_del(&self, key: &str) {
        if let Some(pool) = &self.pool {
            if let Ok(mut conn) = pool.get_conn().await {
                let _: Option<()> = redis::cmd("DEL")
                    .arg(key)
                    .query_async(&mut conn)
                    .await
                    .ok();
            }
        }
    }
}
