// 数据库迁移

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

    // 运行迁移
    if current_version < 1 {
        migration_v1(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (1, datetime('now'))",
            [],
        )?;
    }

    if current_version < 2 {
        migration_v2(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (2, datetime('now'))",
            [],
        )?;
    }

    if current_version < 3 {
        migration_v3(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (3, datetime('now'))",
            [],
        )?;
    }

    if current_version < 4 {
        migration_v4(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (4, datetime('now'))",
            [],
        )?;
    }

    if current_version < 5 {
        migration_v5(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (5, datetime('now'))",
            [],
        )?;
    }

    if current_version < 6 {
        migration_v6(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (6, datetime('now'))",
            [],
        )?;
    }

    if current_version < 7 {
        migration_v7(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (7, datetime('now'))",
            [],
        )?;
    }

    if current_version < 8 {
        migration_v8(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (8, datetime('now'))",
            [],
        )?;
    }

    if current_version < 9 {
        migration_v9(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (9, datetime('now'))",
            [],
        )?;
    }

    if current_version < 10 {
        migration_v10(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (10, datetime('now'))",
            [],
        )?;
    }

    if current_version < 11 {
        migration_v11(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (11, datetime('now'))",
            [],
        )?;
    }

    if current_version < 12 {
        migration_v12(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (12, datetime('now'))",
            [],
        )?;
    }

    if current_version < 13 {
        migration_v13(db)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (13, datetime('now'))",
            [],
        )?;
    }

    Ok(())
}

fn migration_v1(db: &Database) -> Result<()> {
    // Providers 表
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
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        [],
    )?;

    // Provider Endpoints 表
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

    // MCP Servers 表
    db.execute(
        "CREATE TABLE mcp_servers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            command TEXT NOT NULL,
            args TEXT,
            env TEXT,
            enabled_claude INTEGER DEFAULT 0,
            enabled_codex INTEGER DEFAULT 0,
            enabled_gemini INTEGER DEFAULT 0,
            enabled_opencode INTEGER DEFAULT 0,
            enabled_openclaw INTEGER DEFAULT 0
        )",
        [],
    )?;

    // Skills 表
    db.execute(
        "CREATE TABLE skills (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            repo_id INTEGER NOT NULL,
            path TEXT NOT NULL,
            enabled_claude INTEGER DEFAULT 0,
            enabled_codex INTEGER DEFAULT 0,
            enabled_gemini INTEGER DEFAULT 0,
            enabled_opencode INTEGER DEFAULT 0,
            FOREIGN KEY (repo_id) REFERENCES skill_repos(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // Skill Repos 表
    db.execute(
        "CREATE TABLE skill_repos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            url TEXT NOT NULL,
            branch TEXT DEFAULT 'main',
            local_path TEXT NOT NULL
        )",
        [],
    )?;

    Ok(())
}

fn migration_v2(db: &Database) -> Result<()> {
    // Usage Stats 表
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
            FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // Provider Health 表
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

    Ok(())
}

fn migration_v3(db: &Database) -> Result<()> {
    // Skills 表添加 description 列
    db.execute(
        "ALTER TABLE skills ADD COLUMN description TEXT",
        [],
    )?;

    // Skill Repos 表添加 last_synced 列
    db.execute(
        "ALTER TABLE skill_repos ADD COLUMN last_synced TEXT",
        [],
    )?;

    Ok(())
}

fn migration_v4(db: &Database) -> Result<()> {
    // Providers 表添加 provider_type 列
    db.execute(
        "ALTER TABLE providers ADD COLUMN provider_type TEXT NOT NULL DEFAULT 'custom'",
        [],
    )?;

    Ok(())
}

fn migration_v5(db: &Database) -> Result<()> {
    // Providers 表添加 sync_to_cli_tools 列
    db.execute(
        "ALTER TABLE providers ADD COLUMN sync_to_cli_tools TEXT NOT NULL DEFAULT '[]'",
        [],
    )?;

    Ok(())
}

fn migration_v6(db: &Database) -> Result<()> {
    // Providers 表添加 supported_models 列（用于基于 model 参数的智能路由）
    db.execute(
        "ALTER TABLE providers ADD COLUMN supported_models TEXT",
        [],
    )?;

    Ok(())
}

fn migration_v7(db: &Database) -> Result<()> {
    // Providers 表添加 enable_stats 列（是否启用 Token 使用统计）
    db.execute(
        "ALTER TABLE providers ADD COLUMN enable_stats INTEGER NOT NULL DEFAULT 1",
        [],
    )?;

    Ok(())
}

fn migration_v8(db: &Database) -> Result<()> {
    // 添加缓存 token 列到 usage_stats 表
    db.execute(
        "ALTER TABLE usage_stats ADD COLUMN cache_creation_tokens INTEGER DEFAULT 0",
        [],
    )?;
    db.execute(
        "ALTER TABLE usage_stats ADD COLUMN cache_read_tokens INTEGER DEFAULT 0",
        [],
    )?;

    Ok(())
}

fn migration_v9(db: &Database) -> Result<()> {
    // 创建 proxy_request_logs 表用于详细的每请求跟踪
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
            FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // 创建索引以加快查询
    db.execute(
        "CREATE INDEX idx_proxy_request_logs_provider_time
         ON proxy_request_logs(provider_id, request_time)",
        [],
    )?;

    Ok(())
}

fn migration_v10(db: &Database) -> Result<()> {
    // 添加 first_token_ms 字段到 proxy_request_logs 表
    db.execute(
        "ALTER TABLE proxy_request_logs ADD COLUMN first_token_ms INTEGER",
        [],
    )?;

    Ok(())
}

fn migration_v11(db: &Database) -> Result<()> {
    // 添加 session_id 字段到 proxy_request_logs 表
    db.execute(
        "ALTER TABLE proxy_request_logs ADD COLUMN session_id TEXT",
        [],
    )?;

    // 创建索引以加速按 session_id 查询
    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_proxy_request_logs_session_id
         ON proxy_request_logs(session_id)",
        [],
    )?;

    Ok(())
}

fn migration_v12(db: &Database) -> Result<()> {
    // Providers 表添加 api_format 列（用户可覆盖 provider_type 默认的 API 格式）
    db.execute(
        "ALTER TABLE providers ADD COLUMN api_format TEXT",
        [],
    )?;

    Ok(())
}

fn migration_v13(db: &Database) -> Result<()> {
    // Providers 表添加 enable_format_conversion 列（是否启用协议格式转换）
    db.execute(
        "ALTER TABLE providers ADD COLUMN enable_format_conversion INTEGER NOT NULL DEFAULT 0",
        [],
    )?;

    Ok(())
}
