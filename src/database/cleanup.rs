//! 数据库清理和归档功能

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::database::Database;
use crate::models::ProxyRequestLog;
use crate::database::dao::StatsDao;

/// 清理统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupStats {
    /// 清理前的记录总数
    pub total_before: i64,
    /// 归档的记录数
    pub archived_count: i64,
    /// 删除的记录数
    pub deleted_count: i64,
    /// 清理后的记录总数
    pub total_after: i64,
    /// 归档文件路径
    pub archive_file: Option<String>,
    /// 清理耗时（毫秒）
    pub duration_ms: i64,
}

/// 数据库清理器
pub struct DatabaseCleaner<'a> {
    db: &'a Database,
}

impl<'a> DatabaseCleaner<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 获取当前请求日志总数
    pub fn get_log_count(&self) -> Result<i64> {
        let conn = self.db.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM proxy_request_logs",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// 检查是否需要清理
    pub fn needs_cleanup(&self, max_logs: i64) -> Result<bool> {
        let count = self.get_log_count()?;
        Ok(count > max_logs)
    }

    /// 执行清理（归档旧数据并删除）
    ///
    /// # 参数
    /// - `max_logs`: 保留的最大日志数量
    /// - `archive_dir`: 归档目录路径
    ///
    /// # 返回
    /// 清理统计信息
    pub fn cleanup(&self, max_logs: i64, archive_dir: &str) -> Result<CleanupStats> {
        let start_time = std::time::Instant::now();

        // 获取清理前的总数
        let total_before = self.get_log_count()?;

        if total_before <= max_logs {
            tracing::info!("[Cleanup] No cleanup needed: {} <= {}", total_before, max_logs);
            return Ok(CleanupStats {
                total_before,
                archived_count: 0,
                deleted_count: 0,
                total_after: total_before,
                archive_file: None,
                duration_ms: start_time.elapsed().as_millis() as i64,
            });
        }

        let to_delete = total_before - max_logs;
        tracing::info!("[Cleanup] Starting cleanup: total={}, max={}, to_delete={}",
            total_before, max_logs, to_delete);

        // 1. 获取要删除的记录（最旧的记录）
        let old_logs = self.get_oldest_logs(to_delete)?;
        let archived_count = old_logs.len() as i64;

        // 2. 归档到文件
        let archive_file = if !old_logs.is_empty() {
            Some(self.archive_logs(&old_logs, archive_dir)?)
        } else {
            None
        };

        // 3. 删除旧记录
        let deleted_count = self.delete_oldest_logs(to_delete)?;

        // 4. VACUUM 压缩数据库
        tracing::info!("[Cleanup] Running VACUUM to reclaim space...");
        self.db.execute("VACUUM", [])?;

        let total_after = self.get_log_count()?;
        let duration_ms = start_time.elapsed().as_millis() as i64;

        tracing::info!("[Cleanup] Cleanup completed: archived={}, deleted={}, duration={}ms",
            archived_count, deleted_count, duration_ms);

        Ok(CleanupStats {
            total_before,
            archived_count,
            deleted_count,
            total_after,
            archive_file,
            duration_ms,
        })
    }

    /// 获取最旧的 N 条记录
    fn get_oldest_logs(&self, limit: i64) -> Result<Vec<ProxyRequestLog>> {
        StatsDao::get_recent_request_logs(self.db, None, limit)
            .map(|mut logs| {
                // 反转顺序，获取最旧的记录
                logs.reverse();
                logs
            })
    }

    /// 归档日志到文件
    fn archive_logs(&self, logs: &[ProxyRequestLog], archive_dir: &str) -> Result<String> {
        // 展开 ~ 路径
        let archive_dir = shellexpand::tilde(archive_dir).to_string();

        // 创建归档目录
        fs::create_dir_all(&archive_dir)?;

        // 生成归档文件名（使用时间戳）
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("request_logs_{}.json", timestamp);
        let filepath = Path::new(&archive_dir).join(&filename);

        // 序列化为 JSON
        let json = serde_json::to_string_pretty(logs)?;

        // 写入文件
        fs::write(&filepath, json)?;

        tracing::info!("[Cleanup] Archived {} logs to: {}", logs.len(), filepath.display());

        Ok(filepath.to_string_lossy().to_string())
    }

    /// 删除最旧的 N 条记录
    fn delete_oldest_logs(&self, limit: i64) -> Result<i64> {
        let deleted = self.db.execute(
            "DELETE FROM proxy_request_logs
             WHERE id IN (
                 SELECT id FROM proxy_request_logs
                 ORDER BY request_time ASC
                 LIMIT ?1
             )",
            [limit],
        )?;

        tracing::info!("[Cleanup] Deleted {} old logs", deleted);

        Ok(deleted as i64)
    }

    /// 获取数据库文件大小（字节）
    pub fn get_db_size(&self) -> Result<i64> {
        let conn = self.db.conn.lock().unwrap();
        let size: i64 = conn.query_row(
            "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
            [],
            |row| row.get(0),
        )?;
        Ok(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use tempfile::tempdir;

    #[test]
    fn test_cleanup_stats_serialization() {
        let stats = CleanupStats {
            total_before: 1500000,
            archived_count: 500000,
            deleted_count: 500000,
            total_after: 1000000,
            archive_file: Some("/path/to/archive.json".to_string()),
            duration_ms: 5000,
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("1500000"));
        assert!(json.contains("500000"));
    }

    #[test]
    fn test_get_log_count() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::new(db_path).unwrap();

        let cleaner = DatabaseCleaner::new(&db);
        let count = cleaner.get_log_count().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_needs_cleanup() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::new(db_path).unwrap();

        let cleaner = DatabaseCleaner::new(&db);
        let needs = cleaner.needs_cleanup(1000000).unwrap();
        assert!(!needs);
    }
}
