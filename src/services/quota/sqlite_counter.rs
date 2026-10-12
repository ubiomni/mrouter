//! SQLite quota counter implementation

use anyhow::Result;
use async_trait::async_trait;
use crate::database::Database;

pub struct SqliteQuotaCounter {
    db: Database,
}

impl SqliteQuotaCounter {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl super::counter::QuotaCounter for SqliteQuotaCounter {
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

        #[cfg(feature = "sqlite")]
        {
            self.db.execute(
                "INSERT INTO quota_counters (rule_id, subject_key, model, window_start, counter)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(rule_id, subject_key, model, window_start)
                 DO UPDATE SET counter = counter + ?5",
                rusqlite::params![rule_id, subject_key, model, window_start_dt, amount],
            )?;

            let conn = self.db.conn.lock().unwrap();
            let count: i64 = conn.query_row(
                "SELECT counter FROM quota_counters
                 WHERE rule_id=?1 AND subject_key=?2 AND model=?3 AND window_start=?4",
                rusqlite::params![rule_id, subject_key, model, window_start_dt],
                |row| row.get(0),
            )?;
            Ok(count)
        }

        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (rule_id, subject_key, model, window_start_dt, amount);
            Ok(0)
        }
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

        #[cfg(feature = "sqlite")]
        {
            let conn = self.db.conn.lock().unwrap();
            let count: i64 = conn.query_row(
                "SELECT counter FROM quota_counters
                 WHERE rule_id=?1 AND subject_key=?2 AND model=?3 AND window_start=?4",
                rusqlite::params![rule_id, subject_key, model, window_start_dt],
                |row| row.get(0),
            ).unwrap_or(0);
            Ok(count)
        }

        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (rule_id, subject_key, model, window_start_dt);
            Ok(0)
        }
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
        self.increment(rule_id, subject_key, model, now, window_secs, amount).await?;

        let window_start_ts = now - window_secs;
        let window_start_dt = chrono::DateTime::from_timestamp(window_start_ts, 0)
            .unwrap()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        #[cfg(feature = "sqlite")]
        {
            let conn = self.db.conn.lock().unwrap();
            let total: i64 = conn.query_row(
                "SELECT COALESCE(SUM(counter), 0) FROM quota_counters
                 WHERE rule_id=?1 AND subject_key=?2 AND model=?3 AND window_start >= ?4",
                rusqlite::params![rule_id, subject_key, model, window_start_dt],
                |row| row.get(0),
            ).unwrap_or(0);
            Ok(total)
        }

        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (rule_id, subject_key, model, window_start_dt);
            Ok(0)
        }
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

        #[cfg(feature = "sqlite")]
        {
            let conn = self.db.conn.lock().unwrap();
            let total: i64 = conn.query_row(
                "SELECT COALESCE(SUM(counter), 0) FROM quota_counters
                 WHERE rule_id=?1 AND subject_key=?2 AND model=?3 AND window_start >= ?4",
                rusqlite::params![rule_id, subject_key, model, window_start_dt],
                |row| row.get(0),
            ).unwrap_or(0);
            Ok(total)
        }

        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (rule_id, subject_key, model, window_start_dt);
            Ok(0)
        }
    }
}
