// 健康检查服务

use anyhow::Result;
use chrono::Utc;
use std::time::{Duration, Instant};
use crate::database::Database;
use crate::models::{Provider, ProviderHealth};

/// 健康检查服务
pub struct HealthCheckService {
    db: Database,
}

impl HealthCheckService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 检查单个 Provider 的健康状态
    pub async fn check_provider(&self, provider: &Provider) -> Result<ProviderHealth> {
        let start = Instant::now();

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        let test_url = format!("{}/v1/messages", provider.base_url);

        let result = client
            .post(&test_url)
            .header("x-api-key", &provider.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&serde_json::json!({
                "model": "claude-3-opus-20240229",
                "max_tokens": 1,
                "messages": [{
                    "role": "user",
                    "content": "test"
                }]
            }))
            .send()
            .await;

        let latency_ms = start.elapsed().as_millis() as u64;

        let (is_healthy, last_error) = match result {
            Ok(response) => {
                let status = response.status();
                if status.is_success() || status.as_u16() == 400 {
                    (true, None)
                } else {
                    (false, Some(format!("HTTP {}", status)))
                }
            }
            Err(e) => {
                (false, Some(e.to_string()))
            }
        };

        let success_rate = self.calculate_success_rate(provider.id)?;

        let consecutive_failures = if is_healthy {
            0
        } else {
            self.get_consecutive_failures(provider.id)? + 1
        };

        let health = ProviderHealth {
            provider_id: provider.id,
            is_healthy,
            latency_ms: Some(latency_ms),
            success_rate,
            last_error,
            last_check: Utc::now().timestamp_millis(),
            consecutive_failures,
        };

        self.save_health_status(&health)?;

        Ok(health)
    }

    /// 检查所有 Provider
    pub async fn check_all_providers(&self, providers: &[Provider]) -> Result<Vec<ProviderHealth>> {
        let mut results = Vec::new();

        for provider in providers {
            match self.check_provider(provider).await {
                Ok(health) => results.push(health),
                Err(e) => {
                    tracing::error!("Failed to check provider {}: {}", provider.name, e);
                }
            }
        }

        Ok(results)
    }

    // ---- SQLite implementations ----

    #[cfg(feature = "sqlite")]
    fn calculate_success_rate(&self, provider_id: i64) -> Result<f64> {
        let conn = self.db.conn.lock().unwrap();

        let (total, successful): (i64, i64) = conn.query_row(
            "SELECT COUNT(*), SUM(CASE WHEN is_healthy = 1 THEN 1 ELSE 0 END)
             FROM provider_health
             WHERE provider_id = ?1
             AND last_check > datetime('now', '-24 hours')",
            [provider_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap_or((0, 0));

        if total > 0 {
            Ok(successful as f64 / total as f64)
        } else {
            Ok(1.0)
        }
    }

    #[cfg(feature = "sqlite")]
    fn get_consecutive_failures(&self, provider_id: i64) -> Result<i32> {
        let conn = self.db.conn.lock().unwrap();

        let failures: i32 = conn.query_row(
            "SELECT COALESCE(consecutive_failures, 0)
             FROM provider_health
             WHERE provider_id = ?1
             ORDER BY last_check DESC
             LIMIT 1",
            [provider_id],
            |row| row.get(0),
        ).unwrap_or(0);

        Ok(failures)
    }

    #[cfg(feature = "sqlite")]
    fn save_health_status(&self, health: &ProviderHealth) -> Result<()> {
        self.db.execute(
            "INSERT INTO provider_health (provider_id, is_healthy, latency_ms, success_rate, last_error, last_check, consecutive_failures)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                health.provider_id,
                if health.is_healthy { 1 } else { 0 },
                health.latency_ms,
                health.success_rate,
                health.last_error,
                health.last_check,
                health.consecutive_failures,
            ],
        )?;
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    pub fn get_latest_health(&self, provider_id: i64) -> Result<Option<ProviderHealth>> {
        let conn = self.db.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT provider_id, is_healthy, latency_ms, success_rate, last_error, last_check, consecutive_failures
             FROM provider_health
             WHERE provider_id = ?1
             ORDER BY last_check DESC
             LIMIT 1"
        )?;

        let mut rows = stmt.query([provider_id])?;

        if let Some(row) = rows.next()? {
            Ok(Some(ProviderHealth {
                provider_id: row.get(0)?,
                is_healthy: row.get::<_, i32>(1)? != 0,
                latency_ms: row.get(2)?,
                success_rate: row.get(3)?,
                last_error: row.get(4)?,
                last_check: row.get(5)?,
                consecutive_failures: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    #[cfg(feature = "sqlite")]
    pub fn cleanup_old_records(&self, days: i64) -> Result<usize> {
        let deleted = self.db.execute(
            "DELETE FROM provider_health WHERE last_check < datetime('now', ?1)",
            [format!("-{} days", days)],
        )?;

        tracing::info!("Cleaned up {} old health check records", deleted);
        Ok(deleted)
    }

    #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
    fn calculate_success_rate(&self, provider_id: i64) -> Result<f64> {
        let cutoff = chrono::Utc::now().timestamp_millis() - 86400;
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row: (i64, i64) = sqlx::query_as(
                    "SELECT COUNT(*), COALESCE(SUM(CASE WHEN is_healthy = 1 THEN 1 ELSE 0 END), 0)
                     FROM provider_health
                     WHERE provider_id = ?
                     AND last_check > ?"
                ).bind(provider_id).bind(cutoff).fetch_one(&self.db.pool).await
                    .unwrap_or((0, 0));

                if row.0 > 0 { Ok(row.1 as f64 / row.0 as f64) } else { Ok(1.0) }
            })
        })
    }

    #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
    fn get_consecutive_failures(&self, provider_id: i64) -> Result<i32> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row: Option<(i32,)> = sqlx::query_as(
                    "SELECT COALESCE(consecutive_failures, 0)
                     FROM provider_health
                     WHERE provider_id = ?
                     ORDER BY last_check DESC
                     LIMIT 1"
                ).bind(provider_id).fetch_optional(&self.db.pool).await?;
                Ok(row.map(|r| r.0).unwrap_or(0))
            })
        })
    }

    #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
    fn save_health_status(&self, health: &ProviderHealth) -> Result<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query(
                    "INSERT INTO provider_health (provider_id, is_healthy, latency_ms, success_rate, last_error, last_check, consecutive_failures)
                     VALUES (?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(health.provider_id)
                .bind(if health.is_healthy { 1i8 } else { 0 })
                .bind(health.latency_ms)
                .bind(health.success_rate)
                .bind(&health.last_error)
                .bind(&health.last_check)
                .bind(health.consecutive_failures)
                .execute(&self.db.pool).await?;
                Ok(())
            })
        })
    }

    #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
    pub fn get_latest_health(&self, provider_id: i64) -> Result<Option<ProviderHealth>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row = sqlx::query_as::<_, (i64, i8, Option<i64>, f64, Option<String>, i64, i32)>(
                    "SELECT provider_id, is_healthy, latency_ms, success_rate, last_error, last_check, consecutive_failures
                     FROM provider_health WHERE provider_id = ? ORDER BY last_check DESC LIMIT 1"
                ).bind(provider_id).fetch_optional(&self.db.pool).await?;

                Ok(row.map(|r| ProviderHealth {
                    provider_id: r.0,
                    is_healthy: r.1 != 0,
                    latency_ms: r.2.map(|v| v as u64),
                    success_rate: r.3,
                    last_error: r.4,
                    last_check: r.5,
                    consecutive_failures: r.6,
                }))
            })
        })
    }

    #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
    pub fn cleanup_old_records(&self, days: i64) -> Result<usize> {
        let cutoff = chrono::Utc::now().timestamp_millis() - (days * 86400);
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let result = sqlx::query(
                    "DELETE FROM provider_health WHERE last_check < ?"
                ).bind(cutoff).execute(&self.db.pool).await?;
                let deleted = result.rows_affected() as usize;
                tracing::info!("Cleaned up {} old health check records", deleted);
                Ok(deleted)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check() {
        // 测试健康检查逻辑
    }
}
