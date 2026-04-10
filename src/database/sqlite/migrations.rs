// SQLite 数据库迁移（合并版 - 单个 V1 包含完整 schema）

use anyhow::Result;
use super::Database;

pub fn run_migrations(db: &Database) -> Result<()> {
    // 创建版本表
    db.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        )",
        [],
    )?;

    // 获取当前版本
    let current_version: i32 = db
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // 新环境：直接创建完整 schema
    if current_version < 1 {
        migration_v1(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (1, datetime('now'))",
            [],
        )?;
    }

    Ok(())
}

/// 完整 schema（合并自原 V1-V17）
fn migration_v1(db: &Database) -> Result<()> {
    // ── providers ──
    db.execute(
        "CREATE TABLE providers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            app_type TEXT NOT NULL,
            name TEXT NOT NULL,
            is_active INTEGER DEFAULT 0,
            api_key TEXT NOT NULL,
            base_url TEXT NOT NULL,
            model TEXT,
            config TEXT NOT NULL,
            priority INTEGER DEFAULT 0,
            provider_type TEXT NOT NULL DEFAULT 'custom',
            sync_to_cli_tools TEXT NOT NULL DEFAULT '[]',
            supported_models TEXT,
            enable_stats INTEGER NOT NULL DEFAULT 1,
            api_format TEXT,
            enable_format_conversion INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        [],
    )?;
    db.execute(
        "CREATE UNIQUE INDEX idx_providers_name ON providers(name)",
        [],
    )?;

    // ── provider_endpoints ──
    db.execute(
        "CREATE TABLE provider_endpoints (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            provider_id INTEGER NOT NULL,
            url TEXT NOT NULL,
            priority INTEGER DEFAULT 0,
            is_healthy INTEGER DEFAULT 1,
            last_check TEXT,
            FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // ── usage_stats ──
    db.execute(
        "CREATE TABLE usage_stats (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            provider_id INTEGER NOT NULL,
            timestamp TEXT NOT NULL,
            request_count INTEGER DEFAULT 0,
            input_tokens INTEGER DEFAULT 0,
            output_tokens INTEGER DEFAULT 0,
            total_tokens INTEGER DEFAULT 0,
            cost REAL DEFAULT 0.0,
            cache_creation_tokens INTEGER DEFAULT 0,
            cache_read_tokens INTEGER DEFAULT 0,
            FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // ── provider_health ──
    db.execute(
        "CREATE TABLE provider_health (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            provider_id INTEGER NOT NULL,
            is_healthy INTEGER DEFAULT 1,
            latency_ms INTEGER,
            success_rate REAL DEFAULT 1.0,
            last_error TEXT,
            last_check TEXT NOT NULL,
            consecutive_failures INTEGER DEFAULT 0,
            FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // ── proxy_request_logs ──
    db.execute(
        "CREATE TABLE proxy_request_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            provider_id INTEGER NOT NULL,
            request_time TEXT NOT NULL,
            response_time TEXT,
            duration_ms INTEGER,
            status_code INTEGER,
            model TEXT,
            input_tokens INTEGER DEFAULT 0,
            output_tokens INTEGER DEFAULT 0,
            cache_creation_tokens INTEGER DEFAULT 0,
            cache_read_tokens INTEGER DEFAULT 0,
            total_tokens INTEGER DEFAULT 0,
            estimated_cost REAL DEFAULT 0.0,
            error_message TEXT,
            request_path TEXT,
            request_method TEXT,
            first_token_ms INTEGER,
            session_id TEXT,
            token_id INTEGER,
            token_name TEXT,
            FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
        )",
        [],
    )?;
    db.execute(
        "CREATE INDEX idx_proxy_request_logs_provider_time
         ON proxy_request_logs(provider_id, request_time)",
        [],
    )?;
    db.execute(
        "CREATE INDEX idx_proxy_request_logs_session_id
         ON proxy_request_logs(session_id)",
        [],
    )?;

    Ok(())
}
