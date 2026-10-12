//! Redis quota counter implementation

#[cfg(feature = "redis")]
use anyhow::Result;
#[cfg(feature = "redis")]
use async_trait::async_trait;
#[cfg(feature = "redis")]
use std::sync::Arc;

#[cfg(feature = "redis")]
pub struct RedisQuotaCounter {
    pool: Arc<crate::utils::redis::RedisPool>,
}

#[cfg(feature = "redis")]
impl RedisQuotaCounter {
    /// Create from a legacy redis URL (kept for backward compatibility).
    pub fn new(redis_url: &str) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            pool: Arc::new(crate::utils::redis::RedisPool::Single(client)),
        })
    }

    /// Create from a shared RedisPool (preferred).
    pub fn new_with_pool(pool: Arc<crate::utils::redis::RedisPool>) -> Result<Self> {
        Ok(Self { pool })
    }

    fn make_key(rule_id: i64, subject_key: &str, model: &str, window_start: i64) -> String {
        format!("quota:{}:{}:{}:{}", rule_id, subject_key, model, window_start)
    }

    fn make_sliding_key(rule_id: i64, subject_key: &str, model: &str) -> String {
        format!("quota_sw:{}:{}:{}", rule_id, subject_key, model)
    }
}

#[cfg(feature = "redis")]
#[async_trait]
impl super::counter::QuotaCounter for RedisQuotaCounter {
    async fn increment(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_start: i64,
        window_secs: i64,
        amount: i64,
    ) -> Result<i64> {
        let mut conn = self.pool.get_conn().await?;
        let key = Self::make_key(rule_id, subject_key, model, window_start);

        let count: i64 = redis::cmd("INCRBY")
            .arg(&key)
            .arg(amount)
            .query_async(&mut conn)
            .await?;

        // Set TTL on first increment
        if count == amount {
            redis::cmd("EXPIRE")
                .arg(&key)
                .arg(window_secs + 60) // extra buffer
                .query_async::<()>(&mut conn)
                .await?;
        }

        Ok(count)
    }

    async fn get(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_start: i64,
    ) -> Result<i64> {
        let mut conn = self.pool.get_conn().await?;
        let key = Self::make_key(rule_id, subject_key, model, window_start);
        let count: Option<i64> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await?;
        Ok(count.unwrap_or(0))
    }

    async fn increment_sliding(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_secs: i64,
        amount: i64,
    ) -> Result<i64> {
        let mut conn = self.pool.get_conn().await?;
        let key = Self::make_sliding_key(rule_id, subject_key, model);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let window_start_ms = now_ms - (window_secs * 1000);
        let rand_suffix: u32 = rand::random();
        let member = format!("{}:{}:{}", now_ms, rand_suffix, amount);

        // Lua: ZREMRANGEBYSCORE + ZADD + sum members + EXPIRE
        let lua = r#"
            local key = KEYS[1]
            local window_start = tonumber(ARGV[1])
            local score = tonumber(ARGV[2])
            local member = ARGV[3]
            local ttl = tonumber(ARGV[4])
            redis.call('ZREMRANGEBYSCORE', key, '-inf', window_start - 1)
            redis.call('ZADD', key, score, member)
            redis.call('EXPIRE', key, ttl)
            local members = redis.call('ZRANGEBYSCORE', key, window_start, '+inf')
            local total = 0
            for i = 1, #members do
                local last_colon = members[i]:find(":[^:]*$")
                if last_colon then
                    total = total + (tonumber(members[i]:sub(last_colon + 1)) or 0)
                end
            end
            return total
        "#;

        let total: i64 = redis::Script::new(lua)
            .key(&key)
            .arg(window_start_ms)
            .arg(now_ms)
            .arg(&member)
            .arg(window_secs + 60)
            .invoke_async(&mut conn)
            .await?;

        Ok(total)
    }

    async fn get_sliding(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_secs: i64,
    ) -> Result<i64> {
        let mut conn = self.pool.get_conn().await?;
        let key = Self::make_sliding_key(rule_id, subject_key, model);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let window_start_ms = now_ms - (window_secs * 1000);

        let lua = r#"
            local key = KEYS[1]
            local window_start = tonumber(ARGV[1])
            redis.call('ZREMRANGEBYSCORE', key, '-inf', window_start - 1)
            local members = redis.call('ZRANGEBYSCORE', key, window_start, '+inf')
            local total = 0
            for i = 1, #members do
                local last_colon = members[i]:find(":[^:]*$")
                if last_colon then
                    total = total + (tonumber(members[i]:sub(last_colon + 1)) or 0)
                end
            end
            return total
        "#;

        let total: i64 = redis::Script::new(lua)
            .key(&key)
            .arg(window_start_ms)
            .invoke_async(&mut conn)
            .await?;

        Ok(total)
    }
}
