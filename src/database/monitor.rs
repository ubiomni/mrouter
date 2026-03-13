//! 数据库性能监控

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::database::Database;

/// 数据库性能指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbPerformanceMetrics {
    /// 数据库文件大小（字节）
    pub db_size_bytes: i64,
    /// 数据库文件大小（MB）
    pub db_size_mb: f64,
    /// 请求日志总数
    pub total_request_logs: i64,
    /// 使用统计总数
    pub total_usage_stats: i64,
    /// Providers 总数
    pub total_providers: i64,
    /// 页面数量
    pub page_count: i64,
    /// 页面大小（字节）
    pub page_size: i64,
    /// 缓存大小（页面数）
    pub cache_size: i64,
    /// WAL 模式
    pub wal_mode: bool,
    /// 同步模式
    pub synchronous: String,
}

/// 查询性能指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPerformance {
    /// 查询描述
    pub query_name: String,
    /// 执行时间（毫秒）
    pub duration_ms: i64,
    /// 返回的行数
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

    /// 获取数据库性能指标
    pub fn get_metrics(&self) -> Result<DbPerformanceMetrics> {
        let conn = self.db.conn.lock().unwrap();

        // 获取数据库大小
        let (page_count, page_size): (i64, i64) = conn.query_row(
            "SELECT page_count, page_size FROM pragma_page_count(), pragma_page_size()",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let db_size_bytes = page_count * page_size;
        let db_size_mb = db_size_bytes as f64 / 1024.0 / 1024.0;

        // 获取记录数
        let total_request_logs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM proxy_request_logs",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        let total_usage_stats: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_stats",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        let total_providers: i64 = conn.query_row(
            "SELECT COUNT(*) FROM providers",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        // 获取缓存大小
        let cache_size: i64 = conn.query_row(
            "PRAGMA cache_size",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        // 获取 WAL 模式
        let journal_mode: String = conn.query_row(
            "PRAGMA journal_mode",
            [],
            |row| row.get(0),
        ).unwrap_or_else(|_| "unknown".to_string());
        let wal_mode = journal_mode.to_lowercase() == "wal";

        // 获取同步模式
        let synchronous: String = conn.query_row(
            "PRAGMA synchronous",
            [],
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
            db_size_bytes,
            db_size_mb,
            total_request_logs,
            total_usage_stats,
            total_providers,
            page_count,
            page_size,
            cache_size,
            wal_mode,
            synchronous,
        })
    }

    /// 测试查询性能
    pub fn benchmark_query(&self, query_name: &str, sql: &str) -> Result<QueryPerformance> {
        let start = Instant::now();
        let conn = self.db.conn.lock().unwrap();

        let mut stmt = conn.prepare(sql)?;
        let rows_returned = stmt.query_map([], |_| Ok(()))?.count() as i64;

        let duration_ms = start.elapsed().as_millis() as i64;

        Ok(QueryPerformance {
            query_name: query_name.to_string(),
            duration_ms,
            rows_returned,
        })
    }

    /// 运行常见查询的性能测试
    pub fn benchmark_common_queries(&self) -> Result<Vec<QueryPerformance>> {
        let mut results = Vec::new();

        // 测试 1: 获取最近 100 条日志
        results.push(self.benchmark_query(
            "Recent 100 logs",
            "SELECT * FROM proxy_request_logs ORDER BY request_time DESC LIMIT 100",
        )?);

        // 测试 2: 按 provider 统计
        results.push(self.benchmark_query(
            "Stats by provider",
            "SELECT provider_id, COUNT(*), SUM(total_tokens), SUM(estimated_cost)
             FROM proxy_request_logs
             GROUP BY provider_id",
        )?);

        // 测试 3: 按日期统计
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

    /// 优化数据库
    pub fn optimize(&self) -> Result<()> {
        tracing::info!("[Monitor] Running database optimization...");

        let conn = self.db.conn.lock().unwrap();

        // 分析表以更新统计信息
        conn.execute("ANALYZE", [])?;
        tracing::info!("[Monitor] ANALYZE completed");

        // 如果不是 WAL 模式，运行 VACUUM
        let journal_mode: String = conn.query_row(
            "PRAGMA journal_mode",
            [],
            |row| row.get(0),
        )?;

        if journal_mode.to_lowercase() != "wal" {
            conn.execute("VACUUM", [])?;
            tracing::info!("[Monitor] VACUUM completed");
        } else {
            // WAL 模式下使用 WAL checkpoint
            conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", [])?;
            tracing::info!("[Monitor] WAL checkpoint completed");
        }

        Ok(())
    }

    /// 获取表的大小信息
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
            db_size_bytes: 1024000,
            db_size_mb: 1.0,
            total_request_logs: 1000,
            total_usage_stats: 100,
            total_providers: 5,
            page_count: 250,
            page_size: 4096,
            cache_size: 2000,
            wal_mode: true,
            synchronous: "NORMAL".to_string(),
        };

        let json = serde_json::to_string(&metrics).unwrap();
        assert!(json.contains("1024000"));
        assert!(json.contains("NORMAL"));
    }
}
