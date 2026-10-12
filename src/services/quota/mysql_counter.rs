//! MySQL quota counter implementation
//! Uses explicit transactions (SELECT FOR UPDATE + INSERT/UPDATE) for portability.

#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
use anyhow::Result;
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
use async_trait::async_trait;
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
use sqlx::Row;

#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
use crate::database::Database;

#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
pub struct MysqlQuotaCounter {
    db: Database,
}

#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
impl MysqlQuotaCounter {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
#[async_trait]
impl super::counter::QuotaCounter for MysqlQuotaCounter {
    async fn increment(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_start: i64,
        _window_secs: i64,
        amount: i64,
    ) -> Result<i64> {
        let window_start_dt = chrono::DateTime::from_timestamp(window_start, 0)
            .unwrap()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        // Use a transaction with SELECT FOR UPDATE to atomically read-modify-write
        let mut tx = self.db.pool.begin().await?;

        let existing = sqlx::query(
            "SELECT id, counter FROM quota_counters
             WHERE rule_id = ? AND subject_key = ? AND model = ? AND window_start = ?
             FOR UPDATE"
        )
        .bind(rule_id)
        .bind(subject_key)
        .bind(model)
        .bind(&window_start_dt)
        .fetch_optional(&mut *tx)
        .await?;

        let new_count = match existing {
            Some(row) => {
                let current: i64 = row.get("counter");
                let new_val = current + amount;
                let id: i64 = row.get("id");
                sqlx::query("UPDATE quota_counters SET counter = ? WHERE id = ?")
                    .bind(new_val)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                new_val
            }
            None => {
                sqlx::query(
                    "INSERT INTO quota_counters (rule_id, subject_key, model, window_start, counter)
                     VALUES (?, ?, ?, ?, ?)"
                )
                .bind(rule_id)
                .bind(subject_key)
                .bind(model)
                .bind(&window_start_dt)
                .bind(amount)
                .execute(&mut *tx)
                .await?;
                amount
            }
        };

        tx.commit().await?;
        Ok(new_count)
    }

    async fn get(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_start: i64,
    ) -> Result<i64> {
        let window_start_dt = chrono::DateTime::from_timestamp(window_start, 0)
            .unwrap()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let row = sqlx::query(
            "SELECT COALESCE(counter, 0) as cnt FROM quota_counters
             WHERE rule_id = ? AND subject_key = ? AND model = ? AND window_start = ?"
        )
        .bind(rule_id)
        .bind(subject_key)
        .bind(model)
        .bind(&window_start_dt)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(row.map(|r| r.get::<i64, _>("cnt")).unwrap_or(0))
    }

    async fn increment_sliding(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_secs: i64,
        amount: i64,
    ) -> Result<i64> {
        let now = chrono::Utc::now().timestamp();

        // Insert current timestamp as bucket key
        self.increment(rule_id, subject_key, model, now, window_secs, amount).await?;

        // Sum all counters within the sliding window
        let window_start_ts = now - window_secs;
        let window_start_dt = chrono::DateTime::from_timestamp(window_start_ts, 0)
            .unwrap()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let row = sqlx::query(
            "SELECT COALESCE(SUM(counter), 0) as total FROM quota_counters
             WHERE rule_id = ? AND subject_key = ? AND model = ? AND window_start >= ?"
        )
        .bind(rule_id)
        .bind(subject_key)
        .bind(model)
        .bind(&window_start_dt)
        .fetch_one(&self.db.pool)
        .await?;

        let total: i64 = row.get("total");
        Ok(total)
    }

    async fn get_sliding(
        &self,
        rule_id: i64,
        subject_key: &str,
        model: &str,
        window_secs: i64,
    ) -> Result<i64> {
        let now = chrono::Utc::now().timestamp();
        let window_start_ts = now - window_secs;
        let window_start_dt = chrono::DateTime::from_timestamp(window_start_ts, 0)
            .unwrap()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let row = sqlx::query(
            "SELECT COALESCE(SUM(counter), 0) as total FROM quota_counters
             WHERE rule_id = ? AND subject_key = ? AND model = ? AND window_start >= ?"
        )
        .bind(rule_id)
        .bind(subject_key)
        .bind(model)
        .bind(&window_start_dt)
        .fetch_one(&self.db.pool)
        .await?;

        let total: i64 = row.get("total");
        Ok(total)
    }

    /// Atomic check-and-increment using a single transaction with row locking.
    /// Only increments if current + amount <= limit.
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
        let window_start_dt = chrono::DateTime::from_timestamp(window_start, 0)
            .unwrap()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let mut tx = self.db.pool.begin().await?;

        let existing = sqlx::query(
            "SELECT id, counter FROM quota_counters
             WHERE rule_id = ? AND subject_key = ? AND model = ? AND window_start = ?
             FOR UPDATE"
        )
        .bind(rule_id)
        .bind(subject_key)
        .bind(model)
        .bind(&window_start_dt)
        .fetch_optional(&mut *tx)
        .await?;

        let current = match &existing {
            Some(row) => row.get::<i64, _>("counter"),
            None => 0,
        };

        if current + amount > limit {
            // Over limit — don't increment, just release the lock
            tx.commit().await?;
            return Ok((false, current));
        }

        // Within limit — increment
        match existing {
            Some(row) => {
                let id: i64 = row.get("id");
                sqlx::query("UPDATE quota_counters SET counter = ? WHERE id = ?")
                    .bind(current + amount)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
            None => {
                sqlx::query(
                    "INSERT INTO quota_counters (rule_id, subject_key, model, window_start, counter)
                     VALUES (?, ?, ?, ?, ?)"
                )
                .bind(rule_id)
                .bind(subject_key)
                .bind(model)
                .bind(&window_start_dt)
                .bind(amount)
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;
        Ok((true, current))
    }
}
