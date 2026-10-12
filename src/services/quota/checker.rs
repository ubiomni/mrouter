//! Quota checker engine — rule loading, matching, checking, recording
//!
//! Cache strategy:
//! - With Redis: cache rules in Redis with 60s TTL, DEL on invalidation → all instances consistent
//! - Without Redis: direct DB query every time (no local cache)

use anyhow::Result;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::database::Database;
use super::counter::QuotaCounter;
use super::types::{QuotaRule, QuotaCheckResult, QuotaContext, UsageEntry};

const RULES_REDIS_KEY: &str = "quota:rules";
const RULES_CACHE_TTL_SECS: u64 = 60;

pub struct QuotaChecker {
    db: Database,
    counter: Arc<dyn QuotaCounter>,
    #[cfg(feature = "redis")]
    redis: Option<::redis::Client>,
}

impl QuotaChecker {
    pub fn new(db: Database, counter: Arc<dyn QuotaCounter>) -> Self {
        Self {
            db,
            counter,
            #[cfg(feature = "redis")]
            redis: None,
        }
    }

    #[cfg(feature = "redis")]
    pub fn with_redis(db: Database, counter: Arc<dyn QuotaCounter>, redis_url: &str) -> Self {
        let redis = ::redis::Client::open(redis_url).ok();
        Self { db, counter, redis }
    }

    /// Invalidate rules cache (DEL from Redis so all instances see fresh data)
    pub fn invalidate_rules_cache(&self) {
        #[cfg(feature = "redis")]
        if let Some(ref client) = self.redis {
            let client = client.clone();
            tokio::spawn(async move {
                if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
                    let _: Option<()> = ::redis::cmd("DEL")
                        .arg(RULES_REDIS_KEY)
                        .query_async(&mut conn)
                        .await
                        .ok();
                }
            });
        }
    }

    /// Load rules — Redis cache first, then DB fallback
    async fn load_rules(&self) -> Result<Vec<QuotaRule>> {
        // Try Redis cache
        #[cfg(feature = "redis")]
        if let Some(rules) = self.redis_get_rules() {
            return Ok(rules);
        }

        // DB query
        let rules = self.load_rules_from_db().await?;

        // Store in Redis
        #[cfg(feature = "redis")]
        self.redis_set_rules(&rules);

        debug!("quota rules loaded from DB, {} rules", rules.len());
        Ok(rules)
    }

    async fn load_rules_from_db(&self) -> Result<Vec<QuotaRule>> {
        #[cfg(feature = "sqlite")]
        {
            let conn = self.db.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, name, quota_group, dimension, model_pattern, limit_type, limit_value,
                        window_secs, enabled, priority, window_tz, window_type, provider_name_pattern, skip_owner
                 FROM quota_rules WHERE enabled = 1 ORDER BY priority ASC",
            )?;

            let rules = stmt.query_map([], |row| {
                Ok(QuotaRule {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    quota_group: row.get(2)?,
                    dimension: row.get(3)?,
                    model_pattern: row.get(4)?,
                    limit_type: row.get(5)?,
                    limit_value: row.get(6)?,
                    window_secs: row.get(7)?,
                    enabled: row.get::<_, i32>(8)? != 0,
                    priority: row.get(9)?,
                    window_tz: row.get(10)?,
                    window_type: row.get::<_, Option<String>>(11)?.unwrap_or_else(|| "fixed".to_string()),
                    provider_name_pattern: row.get(12)?,
                    skip_owner: row.get::<_, i32>(13).unwrap_or(0) != 0,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

            return Ok(rules);
        }

        #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
        {
            use sqlx::Row;
            let rows = sqlx::query(
                "SELECT id, name, quota_group, dimension, model_pattern, limit_type, limit_value,
                        window_secs, enabled, priority, window_tz, window_type, provider_name_pattern, skip_owner
                 FROM quota_rules WHERE enabled = 1 ORDER BY priority ASC"
            )
            .fetch_all(&self.db.pool)
            .await?;

            let rules = rows.iter().map(|row| {
                QuotaRule {
                    id: row.get("id"),
                    name: row.get("name"),
                    quota_group: row.get("quota_group"),
                    dimension: row.get("dimension"),
                    model_pattern: row.get("model_pattern"),
                    limit_type: row.get("limit_type"),
                    limit_value: row.get("limit_value"),
                    window_secs: row.get("window_secs"),
                    enabled: row.get::<i32, _>("enabled") != 0,
                    priority: row.get("priority"),
                    window_tz: row.get("window_tz"),
                    window_type: row.get::<Option<String>, _>("window_type").unwrap_or_else(|| "fixed".to_string()),
                    provider_name_pattern: row.get("provider_name_pattern"),
                    skip_owner: row.get::<i32, _>("skip_owner") != 0,
                }
            }).collect();

            return Ok(rules);
        }

        #[allow(unreachable_code)]
        Ok(vec![])
    }

    /// Check if a request is within quota limits.
    /// Returns Ok(result) where result.allowed indicates if the request should proceed.
    pub async fn check_request(&self, ctx: &QuotaContext) -> Result<QuotaCheckResult> {
        let rules = self.load_rules().await?;
        if rules.is_empty() {
            return Ok(QuotaCheckResult {
                allowed: true,
                rule_name: String::new(),
                limit: 0,
                used: 0,
                remaining: 0,
                reset_at: Utc::now(),
                limit_type: String::new(),
            });
        }

        let matched_rules = self.filter_rules_for_context(&rules, ctx);
        if matched_rules.is_empty() {
            return Ok(QuotaCheckResult {
                allowed: true,
                rule_name: String::new(),
                limit: 0,
                used: 0,
                remaining: 0,
                reset_at: Utc::now(),
                limit_type: String::new(),
            });
        }

        // Check each matching rule — any one exceeding limit means denied
        // Use atomic increment-then-check to avoid race conditions in multi-instance deployments.
        // If denied, rollback the increment.
        let mut incremented_rules: Vec<&QuotaRule> = Vec::new();

        for rule in &matched_rules {
            if rule.limit_type == "requests" {
                let result = self.atomic_check_and_increment(rule, ctx, 1).await?;
                if !result.allowed {
                    // Rollback all previously incremented rules
                    for prev_rule in &incremented_rules {
                        let _ = self.record_single_rule(prev_rule, ctx, -1).await;
                    }
                    return Ok(result);
                }
                incremented_rules.push(rule);
            }
        }

        Ok(QuotaCheckResult {
            allowed: true,
            rule_name: String::new(),
            limit: 0,
            used: 0,
            remaining: 0,
            reset_at: Utc::now(),
            limit_type: "requests".to_string(),
        })
    }

    /// Check token quota BEFORE forwarding (pre-check, using current usage only).
    pub async fn check_tokens(&self, ctx: &QuotaContext) -> Result<QuotaCheckResult> {
        let rules = self.load_rules().await?;
        let matched_rules = self.filter_rules_for_context(&rules, ctx);

        for rule in &matched_rules {
            if rule.limit_type == "tokens" {
                let result = self.check_single_rule(rule, ctx).await?;
                if !result.allowed {
                    return Ok(result);
                }
            }
        }

        Ok(QuotaCheckResult {
            allowed: true,
            rule_name: String::new(),
            limit: 0,
            used: 0,
            remaining: 0,
            reset_at: Utc::now(),
            limit_type: "tokens".to_string(),
        })
    }

    /// Record token usage after response (for all matching token-type rules)
    pub async fn record_tokens(&self, ctx: &QuotaContext, token_count: i64) -> Result<()> {
        if token_count <= 0 {
            return Ok(());
        }

        let rules = self.load_rules().await?;
        let matched_rules = self.filter_rules_for_context(&rules, ctx);

        for rule in &matched_rules {
            if rule.limit_type == "tokens" {
                if let Err(e) = self.record_single_rule(rule, ctx, token_count).await {
                    warn!("Failed to record quota for rule '{}': {}", rule.name, e);
                }
            }
        }

        Ok(())
    }

    /// Query current usage for a subject across all matching rules.
    /// Used by admin API to show real-time usage (reads from the counter backend).
    pub async fn query_usage(&self, subject_key: &str, model_filter: Option<&str>) -> Result<Vec<UsageEntry>> {
        let rules = self.load_rules().await?;
        let mut entries = Vec::new();

        for rule in &rules {
            let counter_model = model_filter.unwrap_or("*");

            let current = if rule.window_secs == 0 {
                self.counter.get(rule.id, subject_key, counter_model, 0).await?
            } else if rule.window_type == "sliding" {
                self.counter.get_sliding(rule.id, subject_key, counter_model, rule.window_secs).await?
            } else {
                let ws = Self::window_start(rule.window_secs, rule.window_tz.as_deref());
                self.counter.get(rule.id, subject_key, counter_model, ws).await?
            };

            if current > 0 || model_filter.is_some() {
                entries.push(UsageEntry {
                    rule_id: rule.id,
                    rule_name: rule.name.clone(),
                    subject_key: subject_key.to_string(),
                    model: counter_model.to_string(),
                    used: current,
                    limit: rule.limit_value,
                    remaining: (rule.limit_value - current).max(0),
                    limit_type: rule.limit_type.clone(),
                    window_secs: rule.window_secs,
                });
            }
        }

        Ok(entries)
    }

    /// Filter rules that apply to this context
    fn filter_rules_for_context<'a>(&self, rules: &'a [QuotaRule], ctx: &QuotaContext) -> Vec<&'a QuotaRule> {
        let quota_group = ctx.quota_group.as_deref();

        let mut matched: Vec<&QuotaRule> = rules.iter()
            .filter(|r| r.quota_group.as_deref() == quota_group)
            .collect();

        // Fallback: if no group-specific rules, try group=NULL (global rules)
        if matched.is_empty() && quota_group.is_some() {
            matched = rules.iter()
                .filter(|r| r.quota_group.is_none())
                .collect();
        }

        // Further filter by model pattern, provider pattern, skip_owner
        matched.into_iter()
            .filter(|r| {
                if r.skip_owner && ctx.is_owner {
                    return false;
                }
                if Self::dimension_uses_model(&r.dimension) && !Self::model_matches(&r.model_pattern, &ctx.model) {
                    return false;
                }
                if let Some(ref pattern) = r.provider_name_pattern {
                    if let Some(ref pname) = ctx.provider_name {
                        if !Self::model_matches(pattern, pname) {
                            return false;
                        }
                    }
                }
                true
            })
            .collect()
    }

    /// Check a single rule's current usage against its limit
    async fn check_single_rule(&self, rule: &QuotaRule, ctx: &QuotaContext) -> Result<QuotaCheckResult> {
        let subject_key = Self::build_subject_key(&rule.dimension, ctx);
        let counter_model = if Self::dimension_uses_model(&rule.dimension) {
            ctx.model.as_str()
        } else {
            "*"
        };

        let current = if rule.window_secs == 0 {
            self.counter.get(rule.id, &subject_key, counter_model, 0).await?
        } else if rule.window_type == "sliding" {
            self.counter.get_sliding(rule.id, &subject_key, counter_model, rule.window_secs).await?
        } else {
            let ws = Self::window_start(rule.window_secs, rule.window_tz.as_deref());
            self.counter.get(rule.id, &subject_key, counter_model, ws).await?
        };

        let allowed = current < rule.limit_value;
        let remaining = (rule.limit_value - current).max(0);
        let reset_at = if rule.window_secs == 0 {
            DateTime::from_timestamp(i64::MAX / 1000, 0).unwrap_or_else(|| Utc::now())
        } else {
            Self::window_reset(rule.window_secs, rule.window_tz.as_deref())
        };

        Ok(QuotaCheckResult {
            allowed,
            rule_name: rule.name.clone(),
            limit: rule.limit_value,
            used: current,
            remaining,
            reset_at,
            limit_type: rule.limit_type.clone(),
        })
    }

    /// Atomically check quota using check-and-increment pattern.
    /// The counter backend handles atomicity (MySQL uses SELECT FOR UPDATE in one transaction).
    /// Only increments if within limit; if over limit, returns denied without writing.
    async fn atomic_check_and_increment(&self, rule: &QuotaRule, ctx: &QuotaContext, amount: i64) -> Result<QuotaCheckResult> {
        let subject_key = Self::build_subject_key(&rule.dimension, ctx);
        let counter_model = if Self::dimension_uses_model(&rule.dimension) {
            ctx.model.as_str()
        } else {
            "*"
        };

        let (allowed, current) = if rule.window_secs == 0 {
            self.counter.check_and_increment(
                rule.id, &subject_key, counter_model, 0, 0, amount, rule.limit_value
            ).await?
        } else if rule.window_type == "sliding" {
            self.counter.check_and_increment_sliding(
                rule.id, &subject_key, counter_model, rule.window_secs, amount, rule.limit_value
            ).await?
        } else {
            let ws = Self::window_start(rule.window_secs, rule.window_tz.as_deref());
            self.counter.check_and_increment(
                rule.id, &subject_key, counter_model, ws, rule.window_secs, amount, rule.limit_value
            ).await?
        };

        let reset_at = if rule.window_secs == 0 {
            DateTime::from_timestamp(i64::MAX / 1000, 0).unwrap_or_else(|| Utc::now())
        } else {
            Self::window_reset(rule.window_secs, rule.window_tz.as_deref())
        };

        let remaining = (rule.limit_value - current).max(0);

        Ok(QuotaCheckResult {
            allowed,
            rule_name: rule.name.clone(),
            limit: rule.limit_value,
            used: current,
            remaining,
            reset_at,
            limit_type: rule.limit_type.clone(),
        })
    }

    /// Record usage for a single rule
    async fn record_single_rule(&self, rule: &QuotaRule, ctx: &QuotaContext, amount: i64) -> Result<()> {
        let subject_key = Self::build_subject_key(&rule.dimension, ctx);
        let counter_model = if Self::dimension_uses_model(&rule.dimension) {
            ctx.model.as_str()
        } else {
            "*"
        };

        if rule.window_secs == 0 {
            self.counter.increment(rule.id, &subject_key, counter_model, 0, 0, amount).await?;
        } else if rule.window_type == "sliding" {
            self.counter.increment_sliding(rule.id, &subject_key, counter_model, rule.window_secs, amount).await?;
        } else {
            let ws = Self::window_start(rule.window_secs, rule.window_tz.as_deref());
            self.counter.increment(rule.id, &subject_key, counter_model, ws, rule.window_secs, amount).await?;
        }

        Ok(())
    }

    /// Build the subject key based on dimension
    fn build_subject_key(dimension: &str, ctx: &QuotaContext) -> String {
        match dimension {
            "user" | "user_model" => format!("user:{}", ctx.user_id),
            "token" | "token_model" => format!("token:{}", ctx.key_id),
            "owner" => format!("owner:{}", ctx.org_id),
            _ => format!("token:{}", ctx.key_id),
        }
    }

    /// Whether dimension requires model matching
    fn dimension_uses_model(dimension: &str) -> bool {
        matches!(dimension, "user_model" | "token_model")
    }

    /// Glob-style model pattern matching
    fn model_matches(pattern: &str, model: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        if pattern.ends_with('*') {
            let prefix = &pattern[..pattern.len() - 1];
            return model.starts_with(prefix);
        }
        pattern == model
    }

    /// Calculate fixed window start timestamp (with timezone alignment)
    fn window_start(window_secs: i64, window_tz: Option<&str>) -> i64 {
        let now = Utc::now().timestamp();
        let tz_offset = window_tz
            .and_then(|tz| tz.parse::<chrono_tz::Tz>().ok())
            .map(|tz| {
                use chrono::{Offset, TimeZone};
                tz.from_utc_datetime(&Utc::now().naive_utc())
                    .offset()
                    .fix()
                    .local_minus_utc() as i64
            })
            .unwrap_or(0);
        let local = now + tz_offset;
        (local / window_secs) * window_secs - tz_offset
    }

    /// Calculate when the current window resets
    fn window_reset(window_secs: i64, window_tz: Option<&str>) -> DateTime<Utc> {
        let start = Self::window_start(window_secs, window_tz);
        DateTime::from_timestamp(start + window_secs, 0).unwrap_or_else(|| Utc::now())
    }

    // --- Redis helpers ---

    #[cfg(feature = "redis")]
    fn redis_get_rules(&self) -> Option<Vec<QuotaRule>> {
        let client = self.redis.as_ref()?;
        let mut conn = client.get_connection().ok()?;
        let data: Option<String> = ::redis::cmd("GET")
            .arg(RULES_REDIS_KEY)
            .query(&mut conn)
            .ok()?;
        data.and_then(|s| serde_json::from_str(&s).ok())
    }

    #[cfg(feature = "redis")]
    fn redis_set_rules(&self, rules: &[QuotaRule]) {
        let Some(client) = &self.redis else { return };
        let Ok(json) = serde_json::to_string(rules) else { return };
        let client = client.clone();
        tokio::spawn(async move {
            if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
                let _: Option<()> = ::redis::cmd("SET")
                    .arg(RULES_REDIS_KEY)
                    .arg(&json)
                    .arg("EX")
                    .arg(RULES_CACHE_TTL_SECS)
                    .query_async(&mut conn)
                    .await
                    .ok();
            }
        });
    }
}
