// Stats DAO

use anyhow::Result;
use crate::database::Database;
use crate::models::{UsageSummary, ProviderUsage, RequestLog, BillingType};

pub struct LogFilters {
    pub provider_id: Option<i64>,
    pub token_name: Option<String>,
    pub model: Option<String>,
    pub status: Option<String>,
    pub user_id: Option<i64>,
}

pub struct StatsDao;

// ============================================================
//  SQLite implementation (synchronous)
// ============================================================
#[cfg(feature = "sqlite")]
impl StatsDao {
    pub fn get_summary(db: &Database, from: i64, to: i64) -> Result<UsageSummary> {
        let conn = db.conn.lock().unwrap();

        let (total_requests, total_input_tokens, total_output_tokens, total_cache_creation, total_cache_read, total_cost): (i64, i64, i64, i64, i64, f64) = conn.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cache_creation_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0),
                COALESCE(SUM(estimated_cost), 0.0)
             FROM request_logs
             WHERE request_time BETWEEN ?1 AND ?2",
            rusqlite::params![from, to],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )?;

        let total_tokens = total_input_tokens + total_output_tokens + total_cache_creation + total_cache_read;

        let avg_cost_per_request = if total_requests > 0 {
            total_cost / total_requests as f64
        } else {
            0.0
        };

        let mut stmt = conn.prepare(
            "SELECT COALESCE(l.provider_name, p.name, CAST(l.provider_id AS TEXT)),
                    COUNT(*) as requests,
                    COALESCE(SUM(l.input_tokens), 0),
                    COALESCE(SUM(l.output_tokens), 0),
                    COALESCE(SUM(l.cache_creation_tokens), 0),
                    COALESCE(SUM(l.cache_read_tokens), 0),
                    COALESCE(SUM(l.input_tokens + l.output_tokens + l.cache_creation_tokens + l.cache_read_tokens), 0),
                    COALESCE(SUM(l.estimated_cost), 0.0)
             FROM request_logs l
             LEFT JOIN providers p ON p.id = l.provider_id
             WHERE l.request_time BETWEEN ?1 AND ?2
             GROUP BY 1
             ORDER BY 7 DESC"
        )?;

        let by_provider = stmt.query_map(rusqlite::params![from, to], |row| {
            Ok(ProviderUsage {
                provider_name: row.get(0)?,
                requests: row.get(1)?,
                input_tokens: row.get(2)?,
                output_tokens: row.get(3)?,
                cache_creation_tokens: row.get(4)?,
                cache_read_tokens: row.get(5)?,
                tokens: row.get(6)?,
                cost: row.get(7)?,
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
            total_cache_creation_tokens: total_cache_creation,
            total_cache_read_tokens: total_cache_read,
            total_tokens,
            total_cost,
            avg_cost_per_request,
            by_provider: provider_list,
        })
    }

    /// Returns hourly request counts between from and to timestamps
    pub fn get_hourly_requests(db: &Database, from: i64, to: i64) -> Result<Vec<serde_json::Value>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT strftime('%H', request_time / 1000, 'unixepoch') as hour,
                    COUNT(*) as count
             FROM request_logs
             WHERE request_time BETWEEN ?1 AND ?2
             GROUP BY hour
             ORDER BY hour"
        )?;

        let rows = stmt.query_map(rusqlite::params![from, to], |row| {
            let hour: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok(serde_json::json!({ "hour": hour, "count": count }))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Returns top N models by request count between from and to timestamps
    pub fn get_top_models(db: &Database, from: i64, to: i64, limit: i64) -> Result<Vec<serde_json::Value>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT COALESCE(model, 'unknown') as model, COUNT(*) as count
             FROM request_logs
             WHERE request_time BETWEEN ?1 AND ?2
             GROUP BY model
             ORDER BY count DESC
             LIMIT ?3"
        )?;

        let rows = stmt.query_map(rusqlite::params![from, to, limit], |row| {
            let model: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok(serde_json::json!({ "model": model, "count": count }))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn insert_request_log(db: &Database, log: &RequestLog) -> Result<i64> {
        db.execute(
            "INSERT INTO request_logs
             (provider_id, request_time, response_time, duration_ms, first_token_ms, status_code, model,
              input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens,
              total_tokens, estimated_cost, error_message, request_path, request_method, session_id,
              token_id, token_name, user_id, api_key_id, provider_name, client_ip, request_id, retry_count,
              billing_type, task_type, task_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)",
            rusqlite::params![
                log.provider_id,
                log.request_time,
                log.response_time,
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
                log.token_id,
                log.token_name,
                log.user_id,
                log.api_key_id,
                log.provider_name,
                log.client_ip,
                log.request_id,
                log.retry_count,
                log.billing_type.to_string(),
                log.task_type,
                log.task_count,
            ],
        )?;

        let conn = db.conn.lock().unwrap();
        Ok(conn.last_insert_rowid())
    }

    pub fn get_recent_request_logs(
        db: &Database,
        provider_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<RequestLog>> {
        let conn = db.conn.lock().unwrap();
        let query = if let Some(pid) = provider_id {
            format!(
                "SELECT id, provider_id, request_time, response_time, duration_ms, first_token_ms,
                        status_code, model, input_tokens, output_tokens, cache_creation_tokens,
                        cache_read_tokens, total_tokens, estimated_cost, error_message,
                        request_path, request_method, session_id, token_id, token_name
                 FROM request_logs
                 WHERE provider_id = {}
                 ORDER BY request_time DESC LIMIT {}",
                pid, limit
            )
        } else {
            format!(
                "SELECT id, provider_id, request_time, response_time, duration_ms, first_token_ms,
                        status_code, model, input_tokens, output_tokens, cache_creation_tokens,
                        cache_read_tokens, total_tokens, estimated_cost, error_message,
                        request_path, request_method, session_id, token_id, token_name
                 FROM request_logs
                 ORDER BY request_time DESC LIMIT {}",
                limit
            )
        };

        let mut stmt = conn.prepare(&query)?;
        let logs = stmt.query_map([], |row| Self::sqlite_row_to_log(row))?;

        let mut result = Vec::new();
        for log in logs {
            result.push(log?);
        }
        Ok(result)
    }

    pub fn get_recent_request_logs_with_offset(
        db: &Database,
        provider_id: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RequestLog>> {
        let conn = db.conn.lock().unwrap();
        let query = if let Some(pid) = provider_id {
            format!(
                "SELECT id, provider_id, request_time, response_time, duration_ms, first_token_ms,
                        status_code, model, input_tokens, output_tokens, cache_creation_tokens,
                        cache_read_tokens, total_tokens, estimated_cost, error_message,
                        request_path, request_method, session_id, token_id, token_name
                 FROM request_logs
                 WHERE provider_id = {}
                 ORDER BY request_time DESC LIMIT {} OFFSET {}",
                pid, limit, offset
            )
        } else {
            format!(
                "SELECT id, provider_id, request_time, response_time, duration_ms, first_token_ms,
                        status_code, model, input_tokens, output_tokens, cache_creation_tokens,
                        cache_read_tokens, total_tokens, estimated_cost, error_message,
                        request_path, request_method, session_id, token_id, token_name
                 FROM request_logs
                 ORDER BY request_time DESC LIMIT {} OFFSET {}",
                limit, offset
            )
        };

        let mut stmt = conn.prepare(&query)?;
        let logs = stmt.query_map([], |row| Self::sqlite_row_to_log(row))?;

        let mut result = Vec::new();
        for log in logs {
            result.push(log?);
        }
        Ok(result)
    }

    pub fn count_request_logs(db: &Database, provider_id: Option<i64>) -> Result<i64> {
        let conn = db.conn.lock().unwrap();
        let query = if let Some(pid) = provider_id {
            format!("SELECT COUNT(*) FROM request_logs WHERE provider_id = {}", pid)
        } else {
            "SELECT COUNT(*) FROM request_logs".to_string()
        };

        let count: i64 = conn.query_row(&query, [], |row| row.get(0))?;
        Ok(count)
    }

    fn sqlite_row_to_log(row: &rusqlite::Row<'_>) -> rusqlite::Result<RequestLog> {
        use crate::models::BillingType;
        Ok(RequestLog {
            id: row.get(0)?,
            provider_id: row.get(1)?,
            request_time: row.get(2)?,
            response_time: row.get(3)?,
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
            token_id: row.get(18)?,
            token_name: row.get(19)?,
            user_id: None,
            api_key_id: None,
            api_key_name: None,
            provider_name: None,
            client_ip: None,
            request_id: None,
            retry_count: 0,
            billing_type: BillingType::Token,
            task_type: None,
            task_count: 0,
        })
    }

    fn build_filter_clause_sqlite(filters: &LogFilters) -> (String, Vec<String>) {
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(pid) = filters.provider_id {
            conditions.push("provider_id = ?".to_string());
            params.push(pid.to_string());
        }
        if let Some(ref name) = filters.token_name {
            conditions.push("token_name LIKE ?".to_string());
            params.push(format!("%{}%", name));
        }
        if let Some(ref model) = filters.model {
            conditions.push("model LIKE ?".to_string());
            params.push(format!("%{}%", model));
        }
        if let Some(ref status) = filters.status {
            match status.as_str() {
                "success" => conditions.push("status_code >= 200 AND status_code < 400".to_string()),
                "client_error" => conditions.push("status_code >= 400 AND status_code < 500".to_string()),
                "server_error" => conditions.push("status_code >= 500".to_string()),
                _ => {}
            }
        }
        if let Some(uid) = filters.user_id {
            conditions.push("user_id = ?".to_string());
            params.push(uid.to_string());
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };
        (where_clause, params)
    }

    pub fn get_request_logs_filtered(
        db: &Database,
        filters: &LogFilters,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RequestLog>> {
        let conn = db.conn.lock().unwrap();
        let base_sql = "SELECT id, provider_id, request_time, response_time, duration_ms, first_token_ms,
                        status_code, model, input_tokens, output_tokens, cache_creation_tokens,
                        cache_read_tokens, total_tokens, estimated_cost, error_message,
                        request_path, request_method, session_id, token_id, token_name
                 FROM request_logs";

        let (where_clause, params) = Self::build_filter_clause_sqlite(filters);
        let sql = format!("{}{} ORDER BY request_time DESC LIMIT {} OFFSET {}", base_sql, where_clause, limit, offset);

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
        let logs = stmt.query_map(param_refs.as_slice(), |row| Self::sqlite_row_to_log(row))?;

        let mut result = Vec::new();
        for log in logs {
            result.push(log?);
        }
        Ok(result)
    }

    pub fn count_request_logs_filtered(db: &Database, filters: &LogFilters) -> Result<i64> {
        let conn = db.conn.lock().unwrap();
        let (where_clause, params) = Self::build_filter_clause_sqlite(filters);
        let sql = format!("SELECT COUNT(*) FROM request_logs{}", where_clause);

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
        let count: i64 = conn.query_row(&sql, param_refs.as_slice(), |row| row.get(0))?;
        Ok(count)
    }

}

// ============================================================
//  MySQL implementation (async via block_in_place)
// ============================================================
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
impl StatsDao {
    pub fn get_summary(db: &Database, from: i64, to: i64) -> Result<UsageSummary> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                use sqlx::Row;
                let row = sqlx::query(
                        "SELECT
                            COUNT(*) as cnt,
                            CAST(COALESCE(SUM(input_tokens), 0) AS SIGNED) as sum_input,
                            CAST(COALESCE(SUM(output_tokens), 0) AS SIGNED) as sum_output,
                            CAST(COALESCE(SUM(cache_creation_tokens), 0) AS SIGNED) as sum_cache_c,
                            CAST(COALESCE(SUM(cache_read_tokens), 0) AS SIGNED) as sum_cache_r,
                            CAST(COALESCE(SUM(estimated_cost), 0.0) AS DOUBLE) as sum_cost
                         FROM request_logs
                         WHERE request_time BETWEEN ? AND ?"
                    ).bind(from).bind(to).fetch_one(&db.pool).await?;

                let total_requests: i64 = row.get("cnt");
                let total_input_tokens: i64 = row.get("sum_input");
                let total_output_tokens: i64 = row.get("sum_output");
                let total_cache_creation: i64 = row.get("sum_cache_c");
                let total_cache_read: i64 = row.get("sum_cache_r");
                let total_cost: f64 = row.get("sum_cost");

                let total_tokens = total_input_tokens + total_output_tokens + total_cache_creation + total_cache_read;
                let avg_cost_per_request = if total_requests > 0 { total_cost / total_requests as f64 } else { 0.0 };

                let by_provider: Vec<ProviderUsage> = sqlx::query(
                    "SELECT COALESCE(l.provider_name, CAST(l.provider_id AS CHAR)) as pname,
                            COUNT(*) as requests,
                            CAST(COALESCE(SUM(l.input_tokens), 0) AS SIGNED) as sum_input,
                            CAST(COALESCE(SUM(l.output_tokens), 0) AS SIGNED) as sum_output,
                            CAST(COALESCE(SUM(l.cache_creation_tokens), 0) AS SIGNED) as sum_cache_c,
                            CAST(COALESCE(SUM(l.cache_read_tokens), 0) AS SIGNED) as sum_cache_r,
                            CAST(COALESCE(SUM(l.input_tokens + l.output_tokens + l.cache_creation_tokens + l.cache_read_tokens), 0) AS SIGNED) as sum_tokens,
                            CAST(COALESCE(SUM(l.estimated_cost), 0.0) AS DOUBLE) as sum_cost
                     FROM request_logs l
                     WHERE l.request_time BETWEEN ? AND ?
                     GROUP BY pname
                     ORDER BY sum_tokens DESC"
                ).bind(from).bind(to)
                    .map(|row: sqlx::mysql::MySqlRow| {
                        ProviderUsage {
                            provider_name: row.get("pname"),
                            requests: row.get("requests"),
                            input_tokens: row.get("sum_input"),
                            output_tokens: row.get("sum_output"),
                            cache_creation_tokens: row.get("sum_cache_c"),
                            cache_read_tokens: row.get("sum_cache_r"),
                            tokens: row.get("sum_tokens"),
                            cost: row.get("sum_cost"),
                        }
                    })
                    .fetch_all(&db.pool).await?;

                Ok(UsageSummary {
                    total_requests,
                    total_input_tokens,
                    total_output_tokens,
                    total_cache_creation_tokens: total_cache_creation,
                    total_cache_read_tokens: total_cache_read,
                    total_tokens,
                    total_cost,
                    avg_cost_per_request,
                    by_provider,
                })
            })
        })
    }

    /// Returns hourly request counts between from and to timestamps
    pub fn get_hourly_requests(db: &Database, from: i64, to: i64) -> Result<Vec<serde_json::Value>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows: Vec<(String, i64)> = sqlx::query_as(
                    "SELECT DATE_FORMAT(FROM_UNIXTIME(request_time / 1000), '%H') as hour,
                            COUNT(*) as count
                     FROM request_logs
                     WHERE request_time BETWEEN ? AND ?
                     GROUP BY hour
                     ORDER BY hour"
                ).bind(from).bind(to).fetch_all(&db.pool).await?;

                let result: Vec<serde_json::Value> = rows.into_iter()
                    .map(|(hour, count)| serde_json::json!({ "hour": hour, "count": count }))
                    .collect();
                Ok(result)
            })
        })
    }

    /// Returns top N models by request count between from and to timestamps
    pub fn get_top_models(db: &Database, from: i64, to: i64, limit: i64) -> Result<Vec<serde_json::Value>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows: Vec<(String, i64)> = sqlx::query_as(
                    "SELECT COALESCE(model, 'unknown') as model, COUNT(*) as count
                     FROM request_logs
                     WHERE request_time BETWEEN ? AND ?
                     GROUP BY model
                     ORDER BY count DESC
                     LIMIT ?"
                ).bind(from).bind(to).bind(limit).fetch_all(&db.pool).await?;

                let result: Vec<serde_json::Value> = rows.into_iter()
                    .map(|(model, count)| serde_json::json!({ "model": model, "count": count }))
                    .collect();
                Ok(result)
            })
        })
    }

    pub fn insert_request_log(db: &Database, log: &RequestLog) -> Result<i64> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let result = sqlx::query(
                    "INSERT INTO request_logs
                     (provider_id, request_time, response_time, duration_ms, first_token_ms, status_code, model,
                      input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens,
                      total_tokens, estimated_cost, error_message, request_path, request_method, session_id,
                      token_id, token_name, user_id, api_key_id, api_key_name, provider_name, client_ip, request_id, retry_count,
                      billing_type, task_type, task_count)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(log.provider_id)
                .bind(log.request_time)
                .bind(log.response_time)
                .bind(log.duration_ms)
                .bind(log.first_token_ms)
                .bind(log.status_code)
                .bind(&log.model)
                .bind(log.input_tokens)
                .bind(log.output_tokens)
                .bind(log.cache_creation_tokens)
                .bind(log.cache_read_tokens)
                .bind(log.total_tokens)
                .bind(log.estimated_cost)
                .bind(&log.error_message)
                .bind(&log.request_path)
                .bind(&log.request_method)
                .bind(&log.session_id)
                .bind(log.token_id)
                .bind(&log.token_name)
                .bind(log.user_id)
                .bind(log.api_key_id)
                .bind(&log.api_key_name)
                .bind(&log.provider_name)
                .bind(&log.client_ip)
                .bind(&log.request_id)
                .bind(log.retry_count)
                .bind(log.billing_type.to_string())
                .bind(&log.task_type)
                .bind(log.task_count)
                .execute(&db.pool).await?;

                Ok(result.last_insert_id() as i64)
            })
        })
    }

    pub fn get_recent_request_logs(
        db: &Database,
        provider_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<RequestLog>> {
        Self::get_recent_request_logs_with_offset(db, provider_id, limit, 0)
    }

    pub fn get_recent_request_logs_with_offset(
        db: &Database,
        provider_id: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RequestLog>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let base_sql = "SELECT id, provider_id, request_time, response_time, duration_ms, first_token_ms,
                                status_code, model, input_tokens, output_tokens, cache_creation_tokens,
                                cache_read_tokens, total_tokens, estimated_cost, error_message,
                                request_path, request_method, session_id, token_id, token_name
                         FROM request_logs";

                let rows = if let Some(pid) = provider_id {
                    sqlx::query(&format!("{} WHERE provider_id = ? ORDER BY request_time DESC LIMIT ? OFFSET ?", base_sql))
                        .bind(pid).bind(limit).bind(offset)
                        .map(Self::row_to_log_mysql)
                        .fetch_all(&db.pool).await?
                } else {
                    sqlx::query(&format!("{} ORDER BY request_time DESC LIMIT ? OFFSET ?", base_sql))
                        .bind(limit).bind(offset)
                        .map(Self::row_to_log_mysql)
                        .fetch_all(&db.pool).await?
                };

                Ok(rows)
            })
        })
    }

    fn row_to_log_mysql(row: sqlx::mysql::MySqlRow) -> RequestLog {
        use sqlx::Row;
        use crate::models::BillingType;
        RequestLog {
            id: row.get("id"),
            provider_id: row.get("provider_id"),
            request_time: row.get("request_time"),
            response_time: row.get("response_time"),
            duration_ms: row.get("duration_ms"),
            first_token_ms: row.get("first_token_ms"),
            status_code: row.get("status_code"),
            model: row.get("model"),
            input_tokens: row.get("input_tokens"),
            output_tokens: row.get("output_tokens"),
            cache_creation_tokens: row.get("cache_creation_tokens"),
            cache_read_tokens: row.get("cache_read_tokens"),
            total_tokens: row.get("total_tokens"),
            estimated_cost: row.get("estimated_cost"),
            error_message: row.get("error_message"),
            request_path: row.get("request_path"),
            request_method: row.get("request_method"),
            session_id: row.get("session_id"),
            token_id: row.get("token_id"),
            token_name: row.get("token_name"),
            user_id: None,
            api_key_id: None,
            api_key_name: None,
            provider_name: None,
            client_ip: None,
            request_id: None,
            retry_count: 0,
            billing_type: BillingType::Token,
            task_type: None,
            task_count: 0,
        }
    }

    pub fn count_request_logs(db: &Database, provider_id: Option<i64>) -> Result<i64> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let count: (i64,) = if let Some(pid) = provider_id {
                    sqlx::query_as("SELECT COUNT(*) FROM request_logs WHERE provider_id = ?")
                        .bind(pid).fetch_one(&db.pool).await?
                } else {
                    sqlx::query_as("SELECT COUNT(*) FROM request_logs")
                        .fetch_one(&db.pool).await?
                };
                Ok(count.0)
            })
        })
    }

    /// Build WHERE clause from LogFilters
    fn build_filter_clause(filters: &LogFilters) -> (String, Vec<String>) {
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(pid) = filters.provider_id {
            conditions.push("provider_id = ?".to_string());
            params.push(pid.to_string());
        }
        if let Some(ref name) = filters.token_name {
            conditions.push("token_name LIKE ?".to_string());
            params.push(format!("%{}%", name));
        }
        if let Some(ref model) = filters.model {
            conditions.push("model LIKE ?".to_string());
            params.push(format!("%{}%", model));
        }
        if let Some(ref status) = filters.status {
            match status.as_str() {
                "success" => conditions.push("status_code >= 200 AND status_code < 400".to_string()),
                "client_error" => conditions.push("status_code >= 400 AND status_code < 500".to_string()),
                "server_error" => conditions.push("status_code >= 500".to_string()),
                _ => {}
            }
        }
        if let Some(uid) = filters.user_id {
            conditions.push("user_id = ?".to_string());
            params.push(uid.to_string());
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };
        (where_clause, params)
    }

    pub fn get_request_logs_filtered(
        db: &Database,
        filters: &LogFilters,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RequestLog>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let base_sql = "SELECT id, provider_id, request_time, response_time, duration_ms, first_token_ms,
                                status_code, model, input_tokens, output_tokens, cache_creation_tokens,
                                cache_read_tokens, total_tokens, estimated_cost, error_message,
                                request_path, request_method, session_id, token_id, token_name,
                                user_id, api_key_id, api_key_name, provider_name, client_ip, request_id
                         FROM request_logs";

                let (where_clause, params) = Self::build_filter_clause(filters);
                let sql = format!("{}{} ORDER BY request_time DESC LIMIT ? OFFSET ?", base_sql, where_clause);

                let mut query = sqlx::query(&sql);
                for p in &params {
                    query = query.bind(p);
                }
                query = query.bind(limit).bind(offset);

                let rows = query.map(|row: sqlx::mysql::MySqlRow| {
                    use sqlx::Row;
                    RequestLog {
                        id: row.get("id"),
                        provider_id: row.get("provider_id"),
                        request_time: row.get("request_time"),
                        response_time: row.get("response_time"),
                        duration_ms: row.get("duration_ms"),
                        first_token_ms: row.get("first_token_ms"),
                        status_code: row.get("status_code"),
                        model: row.get("model"),
                        input_tokens: row.get("input_tokens"),
                        output_tokens: row.get("output_tokens"),
                        cache_creation_tokens: row.get("cache_creation_tokens"),
                        cache_read_tokens: row.get("cache_read_tokens"),
                        total_tokens: row.get("total_tokens"),
                        estimated_cost: row.get("estimated_cost"),
                        error_message: row.get("error_message"),
                        request_path: row.get("request_path"),
                        request_method: row.get("request_method"),
                        session_id: row.get("session_id"),
                        token_id: row.get("token_id"),
                        token_name: row.get("token_name"),
                        user_id: row.get("user_id"),
                        api_key_id: row.get("api_key_id"),
                        api_key_name: row.get("api_key_name"),
                        provider_name: row.get("provider_name"),
                        client_ip: row.get("client_ip"),
                        request_id: row.get("request_id"),
                        retry_count: row.try_get("retry_count").unwrap_or(0),
                        billing_type: {
                            let bt: Option<String> = row.try_get("billing_type").ok();
                            match bt.as_deref() {
                                Some("request") => BillingType::Request,
                                Some("task") => BillingType::Task,
                                _ => BillingType::Token,
                            }
                        },
                        task_type: row.try_get("task_type").ok(),
                        task_count: row.try_get("task_count").unwrap_or(0),
                    }
                }).fetch_all(&db.pool).await?;
                Ok(rows)
            })
        })
    }

    pub fn count_request_logs_filtered(db: &Database, filters: &LogFilters) -> Result<i64> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let (where_clause, params) = Self::build_filter_clause(filters);
                let sql = format!("SELECT COUNT(*) FROM request_logs{}", where_clause);

                let mut query = sqlx::query_as::<_, (i64,)>(&sql);
                for p in &params {
                    query = query.bind(p);
                }
                let count = query.fetch_one(&db.pool).await?;
                Ok(count.0)
            })
        })
    }
}
