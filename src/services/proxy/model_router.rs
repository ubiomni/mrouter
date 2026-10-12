//! 模型路由过滤 — 按 provider_models 表匹配 provider
//!
//! 对齐 new-api 的 GetRandomSatisfiedChannel 逻辑：
//! 先按 model 筛出支持的 providers，再在子集内做优先级/权重选择
//!
//! 缓存策略（与 ProviderCache 一致）：
//! - 有 Redis: 缓存到 Redis，修改时 DEL key，所有实例即时一致
//! - 无 Redis: 直接查 MySQL/SQLite，共享 DB 天然一致

use std::collections::HashMap;
use crate::database::Database;
use crate::database::dao::ProviderModelDao;
use crate::models::{Provider, ProviderModel};

/// Redis cache TTL for provider model lists (10 minutes)
const CACHE_TTL_SECS: u64 = 600;

/// 匹配类型（按优先级从高到低）
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModelMatchType {
    /// 精确匹配: model_pattern == requested_model
    Exact = 0,
    /// 前缀通配: "claude-*" matches "claude-sonnet-4-20250514"
    Prefix = 1,
    /// 全通配: "*"
    Wildcard = 2,
    /// Provider 无任何 provider_models 记录，视为支持所有模型
    NoConfig = 3,
}

/// 匹配结果
pub struct ProviderModelMatch {
    pub provider: Provider,
    /// 匹配到的 provider_model 记录（NoConfig 时为 None）
    pub matched_model: Option<ProviderModel>,
    pub match_type: ModelMatchType,
}

/// 模型路由缓存
///
/// - 有 Redis: provider_models 缓存到 Redis，所有实例共享
/// - 无 Redis: 直接查 DB
#[derive(Clone)]
pub struct ModelRouterCache {
    db: Database,
    #[cfg(feature = "redis")]
    redis: Option<redis::Client>,
}

impl ModelRouterCache {
    pub fn new(db: Database, _redis_url: Option<&str>) -> Self {
        #[cfg(feature = "redis")]
        let redis = _redis_url.and_then(|url| {
            redis::Client::open(url)
                .map_err(|e| {
                    tracing::error!("[ModelRouterCache] Failed to connect to Redis: {}", e);
                    e
                })
                .ok()
        });

        Self {
            db,
            #[cfg(feature = "redis")]
            redis,
        }
    }

    /// Get active provider_models for a provider. Redis first, DB fallback.
    fn get_provider_models(&self, provider_id: i64) -> Vec<ProviderModel> {
        #[cfg(feature = "redis")]
        {
            if let Some(cached) = self.redis_get(&format!("mrouter:pm:{}", provider_id)) {
                return cached;
            }
        }

        let models = ProviderModelDao::list_active_by_provider(&self.db, provider_id)
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "[ModelRouter] Failed to load provider_models for provider #{}: {}",
                    provider_id, e
                );
                vec![]
            });

        #[cfg(feature = "redis")]
        {
            self.redis_set(&format!("mrouter:pm:{}", provider_id), &models);
        }

        models
    }

    /// Invalidate cache for a specific provider (call after admin modifies provider_models)
    pub fn invalidate_provider(&self, provider_id: i64) {
        #[cfg(feature = "redis")]
        self.redis_del(&format!("mrouter:pm:{}", provider_id));
        #[cfg(not(feature = "redis"))]
        let _ = provider_id;
    }

    /// Invalidate all provider model caches (bulk operation)
    pub fn invalidate_all(&self) {
        #[cfg(feature = "redis")]
        self.redis_del_pattern("pm:*");
        // No-op without Redis (DB reads are always fresh)
    }

    /// Filter providers by model using cached provider_models
    pub fn filter_providers_by_model(
        &self,
        providers: Vec<Provider>,
        model: &str,
    ) -> Vec<ProviderModelMatch> {
        let mut results = Vec::new();

        for provider in providers {
            let models = self.get_provider_models(provider.id);

            // No provider_models configured → excluded (must explicitly declare supported models)
            if models.is_empty() {
                tracing::debug!(
                    "[ModelRouter] Provider '{}' skipped: no provider_models configured",
                    provider.name
                );
                continue;
            }

            // Find the best match among this provider's model patterns
            let mut best_match: Option<(ModelMatchType, ProviderModel)> = None;

            for pm in &models {
                if let Some(mt) = match_model_pattern(&pm.model_pattern, model) {
                    let dominated = match &best_match {
                        None => true,
                        Some((existing_type, _)) => mt < *existing_type,
                    };
                    if dominated {
                        best_match = Some((mt.clone(), pm.clone()));
                    }
                    if mt == ModelMatchType::Exact {
                        break;
                    }
                }
            }

            if let Some((match_type, pm)) = best_match {
                tracing::debug!(
                    "[ModelRouter] Provider '{}' matched model '{}' via pattern '{}' ({:?})",
                    provider.name, model, pm.model_pattern, match_type
                );
                results.push(ProviderModelMatch {
                    provider,
                    matched_model: Some(pm),
                    match_type,
                });
            }
            // No match → provider is excluded from candidates
        }

        results
    }

    // --- Redis helpers ---

    #[cfg(feature = "redis")]
    fn redis_get(&self, key: &str) -> Option<Vec<ProviderModel>> {
        let client = self.redis.as_ref()?;
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut conn = client.get_multiplexed_async_connection().await.ok()?;
                let data: Option<String> = redis::cmd("GET")
                    .arg(key)
                    .query_async(&mut conn)
                    .await
                    .ok()?;
                data.and_then(|s| serde_json::from_str(&s).ok())
            })
        })
    }

    #[cfg(feature = "redis")]
    fn redis_set(&self, key: &str, models: &[ProviderModel]) {
        let Some(client) = &self.redis else { return };
        let Ok(json) = serde_json::to_string(models) else { return };
        let client = client.clone();
        let key = key.to_string();
        tokio::spawn(async move {
            if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
                let _: Option<()> = redis::cmd("SET")
                    .arg(&key)
                    .arg(&json)
                    .arg("EX")
                    .arg(CACHE_TTL_SECS)
                    .query_async(&mut conn)
                    .await
                    .ok();
            }
        });
    }

    #[cfg(feature = "redis")]
    fn redis_del(&self, key: &str) {
        let Some(client) = &self.redis else { return };
        let client = client.clone();
        let key = key.to_string();
        tokio::spawn(async move {
            if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
                let _: Option<()> = redis::cmd("DEL")
                    .arg(&key)
                    .query_async(&mut conn)
                    .await
                    .ok();
            }
        });
    }

    #[cfg(feature = "redis")]
    fn redis_del_pattern(&self, pattern: &str) {
        let Some(client) = &self.redis else { return };
        let client = client.clone();
        let pattern = pattern.to_string();
        tokio::spawn(async move {
            if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
                // SCAN + DEL for pattern-based deletion
                let keys: Vec<String> = redis::cmd("KEYS")
                    .arg(&pattern)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or_default();
                for key in keys {
                    let _: Option<()> = redis::cmd("DEL")
                        .arg(&key)
                        .query_async(&mut conn)
                        .await
                        .ok();
                }
            }
        });
    }
}

/// Check if a model_pattern matches the requested model
fn match_model_pattern(pattern: &str, model: &str) -> Option<ModelMatchType> {
    if pattern == model {
        return Some(ModelMatchType::Exact);
    }
    if pattern == "*" {
        return Some(ModelMatchType::Wildcard);
    }
    if pattern.ends_with('*') {
        let prefix = &pattern[..pattern.len() - 1];
        if model.starts_with(prefix) {
            return Some(ModelMatchType::Prefix);
        }
    }
    None
}

/// Build model_mappings from match results: provider_id → upstream_model
pub fn build_model_mappings(matches: &[ProviderModelMatch]) -> HashMap<i64, String> {
    let mut mappings = HashMap::new();
    for m in matches {
        if let Some(ref pm) = m.matched_model {
            if let Some(ref upstream) = pm.upstream_model {
                if !upstream.is_empty() {
                    mappings.insert(m.provider.id, upstream.clone());
                }
            }
        }
    }
    mappings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert_eq!(match_model_pattern("gpt-4o", "gpt-4o"), Some(ModelMatchType::Exact));
        assert_eq!(match_model_pattern("gpt-4o", "gpt-4"), None);
    }

    #[test]
    fn test_prefix_match() {
        assert_eq!(
            match_model_pattern("claude-*", "claude-sonnet-4-20250514"),
            Some(ModelMatchType::Prefix)
        );
        assert_eq!(match_model_pattern("claude-*", "gpt-4o"), None);
        assert_eq!(match_model_pattern("gpt-*", "gpt-4o"), Some(ModelMatchType::Prefix));
    }

    #[test]
    fn test_wildcard_match() {
        assert_eq!(match_model_pattern("*", "anything"), Some(ModelMatchType::Wildcard));
        assert_eq!(match_model_pattern("*", ""), Some(ModelMatchType::Wildcard));
    }

    #[test]
    fn test_no_match() {
        assert_eq!(match_model_pattern("gpt-4o", "claude-3"), None);
        assert_eq!(match_model_pattern("openai-*", "claude-3"), None);
    }
}
