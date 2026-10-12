//! Quota counter backend trait

use anyhow::Result;
use async_trait::async_trait;

/// Backend trait for quota counter storage (SQLite, MySQL, or Redis)
#[async_trait]
pub trait QuotaCounter: Send + Sync {
    /// Atomically increment counter for a fixed window, returning new value.
    async fn increment(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_start: i64,
        window_secs: i64,
        amount: i64,
    ) -> Result<i64>;

    /// Get current counter value for a fixed window.
    async fn get(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_start: i64,
    ) -> Result<i64>;

    /// Record usage in a sliding window, returning total within window.
    async fn increment_sliding(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_secs: i64,
        amount: i64,
    ) -> Result<i64>;

    /// Get total usage within a sliding window.
    async fn get_sliding(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_secs: i64,
    ) -> Result<i64>;

    /// Atomically check-and-increment: only increment if current + amount <= limit.
    /// Returns (allowed, current_value_before_increment).
    /// Default implementation is non-atomic (get then increment) — backends that support
    /// transactions (MySQL) should override for true atomicity.
    async fn check_and_increment(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_start: i64,
        window_secs: i64,
        amount: i64,
        limit: i64,
    ) -> Result<(bool, i64)> {
        let current = self.get(rule_id, subject_key, model, window_start).await?;
        if current + amount > limit {
            return Ok((false, current));
        }
        self.increment(rule_id, subject_key, model, window_start, window_secs, amount).await?;
        Ok((true, current))
    }

    /// Atomically check-and-increment for sliding window.
    /// Default implementation is non-atomic — backends should override for true atomicity.
    async fn check_and_increment_sliding(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_secs: i64,
        amount: i64,
        limit: i64,
    ) -> Result<(bool, i64)> {
        let current = self.get_sliding(rule_id, subject_key, model, window_secs).await?;
        if current + amount > limit {
            return Ok((false, current));
        }
        self.increment_sliding(rule_id, subject_key, model, window_secs, amount).await?;
        Ok((true, current))
    }
}
