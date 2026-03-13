// Stats DAO

use anyhow::Result;
use crate::database::Database;
use crate::models::{UsageStats, UsageSummary, ProviderUsage, ProxyRequestLog};
use chrono::{DateTime, Utc};

pub struct StatsDao;

impl StatsDao {
    pub fn insert(db: &Database, stats: &UsageStats) -> Result<i64> {
        db.execute(
            "INSERT INTO usage_stats (provider_id, timestamp, request_count, input_tokens, output_tokens, total_tokens, cost, cache_creation_tokens, cache_read_tokens)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                stats.provider_id,
                stats.timestamp.to_rfc3339(),
                stats.request_count,
                stats.input_tokens,
                stats.output_tokens,
                stats.total_tokens,
                stats.cost,
                stats.cache_creation_tokens,
                stats.cache_read_tokens,
            ],
        )?;

        let conn = db.conn.lock().unwrap();
        Ok(conn.last_insert_rowid())
    }

    pub fn get_summary(db: &Database, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<UsageSummary> {
        let conn = db.conn.lock().unwrap();

        // 从 proxy_request_logs 直接聚合统计数据（新方法）
        let (total_requests, total_input_tokens, total_output_tokens, total_cache_creation, total_cache_read, total_cost): (i64, i64, i64, i64, i64, f64) = conn.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cache_creation_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0),
                COALESCE(SUM(estimated_cost), 0.0)
             FROM proxy_request_logs
             WHERE request_time BETWEEN ?1 AND ?2",
            rusqlite::params![from.to_rfc3339(), to.to_rfc3339()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )?;

        let total_tokens = total_input_tokens + total_output_tokens + total_cache_creation + total_cache_read;

        let avg_cost_per_request = if total_requests > 0 {
            total_cost / total_requests as f64
        } else {
            0.0
        };

        // 按 provider 统计（从 proxy_request_logs）
        let mut stmt = conn.prepare(
            "SELECT p.name,
                    COUNT(*) as requests,
                    COALESCE(SUM(l.input_tokens), 0) as input_tokens,
                    COALESCE(SUM(l.output_tokens), 0) as output_tokens,
                    COALESCE(SUM(l.input_tokens + l.output_tokens + l.cache_creation_tokens + l.cache_read_tokens), 0) as total_tokens,
                    COALESCE(SUM(l.estimated_cost), 0.0) as cost
             FROM proxy_request_logs l
             JOIN providers p ON p.id = l.provider_id
             WHERE l.request_time BETWEEN ?1 AND ?2
             GROUP BY p.id, p.name
             ORDER BY total_tokens DESC"
        )?;

        let by_provider = stmt.query_map(rusqlite::params![from.to_rfc3339(), to.to_rfc3339()], |row| {
            Ok(ProviderUsage {
                provider_name: row.get(0)?,
                requests: row.get(1)?,
                input_tokens: row.get(2)?,
                output_tokens: row.get(3)?,
                tokens: row.get(4)?,
                cost: row.get(5)?,
            })
        })?;

        let mut provider_list = Vec::new();
        for p in by_provider {
            provider_list.push(p?);
        }

        Ok(UsageSummary {
            total_requests,
            total_input_tokens,
            total_output_tokens,
            total_tokens,
            total_cost,
            avg_cost_per_request,
            by_provider: provider_list,
        })
    }

    pub fn insert_request_log(db: &Database, log: &ProxyRequestLog) -> Result<i64> {
        db.execute(
            "INSERT INTO proxy_request_logs
             (provider_id, request_time, response_time, duration_ms, first_token_ms, status_code, model,
              input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens,
              total_tokens, estimated_cost, error_message, request_path, request_method, session_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            rusqlite::params![
                log.provider_id,
                log.request_time.to_rfc3339(),
                log.response_time.map(|t| t.to_rfc3339()),
                log.duration_ms,
                log.first_token_ms,
                log.status_code,
                log.model,
                log.input_tokens,
                log.output_tokens,
                log.cache_creation_tokens,
                log.cache_read_tokens,
                log.total_tokens,
                log.estimated_cost,
                log.error_message,
                log.request_path,
                log.request_method,
                log.session_id,
            ],
        )?;

        let conn = db.conn.lock().unwrap();
        Ok(conn.last_insert_rowid())
    }

    pub fn get_recent_request_logs(
        db: &Database,
        provider_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<ProxyRequestLog>> {
        let conn = db.conn.lock().unwrap();
        let query = if let Some(pid) = provider_id {
            format!(
                "SELECT id, provider_id, request_time, response_time, duration_ms, first_token_ms,
                        status_code, model, input_tokens, output_tokens, cache_creation_tokens,
                        cache_read_tokens, total_tokens, estimated_cost, error_message,
                        request_path, request_method, session_id
                 FROM proxy_request_logs
                 WHERE provider_id = {}
                 ORDER BY request_time DESC LIMIT {}",
                pid, limit
            )
        } else {
            format!(
                "SELECT id, provider_id, request_time, response_time, duration_ms, first_token_ms,
                        status_code, model, input_tokens, output_tokens, cache_creation_tokens,
                        cache_read_tokens, total_tokens, estimated_cost, error_message,
                        request_path, request_method, session_id
                 FROM proxy_request_logs
                 ORDER BY request_time DESC LIMIT {}",
                limit
            )
        };

        let mut stmt = conn.prepare(&query)?;
        let logs = stmt.query_map([], |row| {
            Ok(ProxyRequestLog {
                id: row.get(0)?,
                provider_id: row.get(1)?,
                request_time: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                    .unwrap().with_timezone(&Utc),
                response_time: row.get::<_, Option<String>>(3)?
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                duration_ms: row.get(4)?,
                first_token_ms: row.get(5)?,
                status_code: row.get(6)?,
                model: row.get(7)?,
                input_tokens: row.get(8)?,
                output_tokens: row.get(9)?,
                cache_creation_tokens: row.get(10)?,
                cache_read_tokens: row.get(11)?,
                total_tokens: row.get(12)?,
                estimated_cost: row.get(13)?,
                error_message: row.get(14)?,
                request_path: row.get(15)?,
                request_method: row.get(16)?,
                session_id: row.get(17)?,
            })
        })?;

        let mut result = Vec::new();
        for log in logs {
            result.push(log?);
        }
        Ok(result)
    }

    /// 获取最近的请求日志（带分页偏移）
    pub fn get_recent_request_logs_with_offset(
        db: &Database,
        provider_id: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ProxyRequestLog>> {
        let conn = db.conn.lock().unwrap();
        let query = if let Some(pid) = provider_id {
            format!(
                "SELECT id, provider_id, request_time, response_time, duration_ms, first_token_ms,
                        status_code, model, input_tokens, output_tokens, cache_creation_tokens,
                        cache_read_tokens, total_tokens, estimated_cost, error_message,
                        request_path, request_method, session_id
                 FROM proxy_request_logs
                 WHERE provider_id = {}
                 ORDER BY request_time DESC LIMIT {} OFFSET {}",
                pid, limit, offset
            )
        } else {
            format!(
                "SELECT id, provider_id, request_time, response_time, duration_ms, first_token_ms,
                        status_code, model, input_tokens, output_tokens, cache_creation_tokens,
                        cache_read_tokens, total_tokens, estimated_cost, error_message,
                        request_path, request_method, session_id
                 FROM proxy_request_logs
                 ORDER BY request_time DESC LIMIT {} OFFSET {}",
                limit, offset
            )
        };

        let mut stmt = conn.prepare(&query)?;
        let logs = stmt.query_map([], |row| {
            Ok(ProxyRequestLog {
                id: row.get(0)?,
                provider_id: row.get(1)?,
                request_time: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                    .unwrap().with_timezone(&Utc),
                response_time: row.get::<_, Option<String>>(3)?
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                duration_ms: row.get(4)?,
                first_token_ms: row.get(5)?,
                status_code: row.get(6)?,
                model: row.get(7)?,
                input_tokens: row.get(8)?,
                output_tokens: row.get(9)?,
                cache_creation_tokens: row.get(10)?,
                cache_read_tokens: row.get(11)?,
                total_tokens: row.get(12)?,
                estimated_cost: row.get(13)?,
                error_message: row.get(14)?,
                request_path: row.get(15)?,
                request_method: row.get(16)?,
                session_id: row.get(17)?,
            })
        })?;

        let mut result = Vec::new();
        for log in logs {
            result.push(log?);
        }
        Ok(result)
    }

    /// 统计请求日志总数
    pub fn count_request_logs(db: &Database, provider_id: Option<i64>) -> Result<i64> {
        let conn = db.conn.lock().unwrap();
        let query = if let Some(pid) = provider_id {
            format!("SELECT COUNT(*) FROM proxy_request_logs WHERE provider_id = {}", pid)
        } else {
            "SELECT COUNT(*) FROM proxy_request_logs".to_string()
        };

        let count: i64 = conn.query_row(&query, [], |row| row.get(0))?;
        Ok(count)
    }
}
