// MySQL 数据库迁移（合并版 - 单个 V1 包含完整 schema）

use anyhow::Result;
use super::Database;

pub async fn run_migrations(db: &Database) -> Result<()> {
    // 创建版本表
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INT PRIMARY KEY,
            applied_at DATETIME NOT NULL
        )"
    ).execute(&db.pool).await?;

    // 获取当前版本
    let current_version: i32 = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT MAX(version) FROM schema_version"
    ).fetch_one(&db.pool).await
        .unwrap_or(None)
        .unwrap_or(0);

    // 新环境：直接创建完整 schema
    if current_version < 1 {
        migration_v1(db).await?;
        sqlx::query("INSERT INTO schema_version (version, applied_at) VALUES (1, NOW())")
            .execute(&db.pool).await?;
    }

    Ok(())
}

/// 完整 schema（合并自原 V1-V17）
async fn migration_v1(db: &Database) -> Result<()> {
    // ── providers ──
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS providers (
            id BIGINT PRIMARY KEY AUTO_INCREMENT,
            app_type VARCHAR(64) NOT NULL,
            name VARCHAR(255) NOT NULL UNIQUE,
            is_active TINYINT DEFAULT 0,
            api_key TEXT NOT NULL,
            base_url TEXT NOT NULL,
            model VARCHAR(255),
            config JSON NOT NULL,
            priority INT DEFAULT 0,
            provider_type VARCHAR(64) NOT NULL DEFAULT 'custom',
            sync_to_cli_tools JSON NOT NULL DEFAULT ('[]'),
            supported_models JSON,
            enable_stats TINYINT NOT NULL DEFAULT 1,
            api_format VARCHAR(64),
            enable_format_conversion TINYINT NOT NULL DEFAULT 0,
            created_at DATETIME NOT NULL,
            updated_at DATETIME NOT NULL
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci"
    ).execute(&db.pool).await?;

    // ── provider_endpoints ──
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS provider_endpoints (
            id BIGINT PRIMARY KEY AUTO_INCREMENT,
            provider_id BIGINT NOT NULL,
            url TEXT NOT NULL,
            priority INT DEFAULT 0,
            is_healthy TINYINT DEFAULT 1,
            last_check DATETIME,
            FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci"
    ).execute(&db.pool).await?;

    // ── usage_stats ──
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS usage_stats (
            id BIGINT PRIMARY KEY AUTO_INCREMENT,
            provider_id BIGINT NOT NULL,
            timestamp DATETIME NOT NULL,
            request_count INT DEFAULT 0,
            input_tokens BIGINT DEFAULT 0,
            output_tokens BIGINT DEFAULT 0,
            total_tokens BIGINT DEFAULT 0,
            cost DOUBLE DEFAULT 0.0,
            cache_creation_tokens BIGINT DEFAULT 0,
            cache_read_tokens BIGINT DEFAULT 0,
            FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci"
    ).execute(&db.pool).await?;

    // ── provider_health ──
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS provider_health (
            id BIGINT PRIMARY KEY AUTO_INCREMENT,
            provider_id BIGINT NOT NULL,
            is_healthy TINYINT DEFAULT 1,
            latency_ms INT,
            success_rate DOUBLE DEFAULT 1.0,
            last_error TEXT,
            last_check DATETIME NOT NULL,
            consecutive_failures INT DEFAULT 0,
            FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci"
    ).execute(&db.pool).await?;

    // ── proxy_request_logs ──
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS proxy_request_logs (
            id BIGINT PRIMARY KEY AUTO_INCREMENT,
            provider_id BIGINT NOT NULL,
            request_time DATETIME(3) NOT NULL,
            response_time DATETIME(3),
            duration_ms INT,
            status_code INT,
            model VARCHAR(255),
            input_tokens BIGINT DEFAULT 0,
            output_tokens BIGINT DEFAULT 0,
            cache_creation_tokens BIGINT DEFAULT 0,
            cache_read_tokens BIGINT DEFAULT 0,
            total_tokens BIGINT DEFAULT 0,
            estimated_cost DOUBLE DEFAULT 0.0,
            error_message TEXT,
            request_path VARCHAR(512),
            request_method VARCHAR(16),
            first_token_ms INT,
            session_id VARCHAR(255),
            token_id BIGINT,
            token_name VARCHAR(100),
            FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE,
            INDEX idx_proxy_request_logs_provider_time (provider_id, request_time),
            INDEX idx_proxy_request_logs_session_id (session_id),
            INDEX idx_proxy_request_logs_token_id (token_id)
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci"
    ).execute(&db.pool).await?;

    Ok(())
}
