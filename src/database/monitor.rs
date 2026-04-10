//! 数据库性能监控

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::database::Database;

/// 数据库性能指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbPerformanceMetrics {
    pub db_size_bytes: i64,
    pub db_size_mb: f64,
    pub total_request_logs: i64,
    pub total_usage_stats: i64,
    pub total_providers: i64,
    pub page_count: i64,
    pub page_size: i64,
    pub cache_size: i64,
    pub wal_mode: bool,
    pub synchronous: String,
}

/// 查询性能指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPerformance {
    pub query_name: String,
    pub duration_ms: i64,
    pub rows_returned: i64,
}

/// 数据库监控器
pub struct DatabaseMonitor<'a> {
    db: &'a Database,
}

impl<'a> DatabaseMonitor<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    #[cfg(feature = "sqlite")]
    pub fn get_metrics(&self) -> Result<DbPerformanceMetrics> {
        let conn = self.db.conn.lock().unwrap();

        let (page_count, page_size): (i64, i64) = conn.query_row(
            "SELECT page_count, page_size FROM pragma_page_count(), pragma_page_size()",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let db_size_bytes = page_count * page_size;
        let db_size_mb = db_size_bytes as f64 / 1024.0 / 1024.0;

        let total_request_logs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM proxy_request_logs", [], |row| row.get(0),
        ).unwrap_or(0);

        let total_usage_stats: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_stats", [], |row| row.get(0),
        ).unwrap_or(0);

        let total_providers: i64 = conn.query_row(
            "SELECT COUNT(*) FROM providers", [], |row| row.get(0),
        ).unwrap_or(0);

        let cache_size: i64 = conn.query_row(
            "PRAGMA cache_size", [], |row| row.get(0),
        ).unwrap_or(0);

        let journal_mode: String = conn.query_row(
            "PRAGMA journal_mode", [], |row| row.get(0),
        ).unwrap_or_else(|_| "unknown".to_string());
        let wal_mode = journal_mode.to_lowercase() == "wal";

        let synchronous: String = conn.query_row(
            "PRAGMA synchronous", [],
            |row| {
                let val: i64 = row.get(0)?;
                Ok(match val {
                    0 => "OFF".to_string(),
                    1 => "NORMAL".to_string(),
                    2 => "FULL".to_string(),
                    3 => "EXTRA".to_string(),
                    _ => format!("{}", val),
                })
            },
        ).unwrap_or_else(|_| "unknown".to_string());

        Ok(DbPerformanceMetrics {
            db_size_bytes, db_size_mb,
            total_request_logs, total_usage_stats, total_providers,
            page_count, page_size, cache_size, wal_mode, synchronous,
        })
    }

    #[cfg(feature = "sqlite")]
    pub fn benchmark_query(&self, query_name: &str, sql: &str) -> Result<QueryPerformance> {
        let start = std::time::Instant::now();
        let conn = self.db.conn.lock().unwrap();

        let mut stmt = conn.prepare(sql)?;
        let rows_returned = stmt.query_map([], |_| Ok(()))?.count() as i64;

        let duration_ms = start.elapsed().as_millis() as i64;

        Ok(QueryPerformance { query_name: query_name.to_string(), duration_ms, rows_returned })
    }

    pub fn benchmark_common_queries(&self) -> Result<Vec<QueryPerformance>> {
        let mut results = Vec::new();

        results.push(self.benchmark_query(
            "Recent 100 logs",
            "SELECT * FROM proxy_request_logs ORDER BY request_time DESC LIMIT 100",
        )?);

        results.push(self.benchmark_query(
            "Stats by provider",
            "SELECT provider_id, COUNT(*), SUM(total_tokens), SUM(estimated_cost)
             FROM proxy_request_logs
             GROUP BY provider_id",
        )?);

        results.push(self.benchmark_query(
            "Stats by date",
            "SELECT DATE(request_time), COUNT(*), SUM(total_tokens)
             FROM proxy_request_logs
             GROUP BY DATE(request_time)
             ORDER BY DATE(request_time) DESC
             LIMIT 30",
        )?);

        Ok(results)
    }

    #[cfg(feature = "sqlite")]
    pub fn optimize(&self) -> Result<()> {
        tracing::info!("[Monitor] Running database optimization...");
        let conn = self.db.conn.lock().unwrap();
        conn.execute("ANALYZE", [])?;
        tracing::info!("[Monitor] ANALYZE completed");

        let journal_mode: String = conn.query_row(
            "PRAGMA journal_mode", [], |row| row.get(0),
        )?;

        if journal_mode.to_lowercase() != "wal" {
            conn.execute("VACUUM", [])?;
            tracing::info!("[Monitor] VACUUM completed");
        } else {
            conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", [])?;
            tracing::info!("[Monitor] WAL checkpoint completed");
        }

        Ok(())
    }

    #[cfg(feature = "sqlite")]
    pub fn get_table_sizes(&self) -> Result<Vec<(String, i64)>> {
        let conn = self.db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name, SUM(pgsize) as size
             FROM dbstat
             WHERE name IN ('proxy_request_logs', 'usage_stats', 'providers')
             GROUP BY name
             ORDER BY size DESC"
        )?;

        let sizes = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut result = Vec::new();
        for size in sizes {
            result.push(size?);
        }
        Ok(result)
    }
}

#[cfg(test)]
#[cfg(feature = "sqlite")]
mod tests {
    use super::*;
    use crate::database::Database;
    use tempfile::tempdir;

    #[test]
    fn test_get_metrics() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::new(db_path).unwrap();

        let monitor = DatabaseMonitor::new(&db);
        let metrics = monitor.get_metrics().unwrap();

        assert!(metrics.db_size_bytes > 0);
        assert!(metrics.page_count > 0);
        assert!(metrics.page_size > 0);
    }

    #[test]
    fn test_metrics_serialization() {
        let metrics = DbPerformanceMetrics {
            db_size_bytes: 1024000, db_size_mb: 1.0,
            total_request_logs: 1000, total_usage_stats: 100, total_providers: 5,
            page_count: 250, page_size: 4096, cache_size: 2000,
            wal_mode: true, synchronous: "NORMAL".to_string(),
        };

        let json = serde_json::to_string(&metrics).unwrap();
        assert!(json.contains("1024000"));
        assert!(json.contains("NORMAL"));
    }
}
