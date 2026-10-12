// MySQL 数据库迁移 — 外部 SQL 文件驱动

use anyhow::Result;
use super::Database;
use tracing::info;

/// 迁移列表：(版本号, 名称, SQL 内容)
/// 使用 include_str! 在编译时嵌入 SQL 文件
fn get_migrations() -> Vec<(i32, &'static str, &'static str)> {
    vec![
        (1, "init_schema", include_str!("../../../migrations/mysql/V001__init_schema.sql")),
        (2, "seed_data", include_str!("../../../migrations/mysql/V002__seed_data.sql")),
        (3, "seed_presets", include_str!("../../../migrations/mysql/V003__seed_presets.sql")),
    ]
}

/// 运行所有待执行的迁移
pub async fn run_migrations(db: &Database) -> Result<()> {
    // 确保版本表存在
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _schema_version (
            version INT PRIMARY KEY,
            name VARCHAR(255),
            applied_at BIGINT NOT NULL
        )"
    ).execute(&db.pool).await?;

    // 兼容旧版本表（缺少 name 列）
    let _ = sqlx::query("ALTER TABLE _schema_version ADD COLUMN name VARCHAR(255)")
        .execute(&db.pool).await;

    // 获取当前已执行的最高版本
    let current_version: i32 = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT MAX(version) FROM _schema_version"
    ).fetch_one(&db.pool).await
        .unwrap_or(None)
        .unwrap_or(0);

    // 按序执行未应用的迁移
    for (version, name, sql) in get_migrations() {
        if version > current_version {
            info!("[Migration] Applying V{:03}_{} ...", version, name);
            execute_migration(db, version, name, sql).await?;
            info!("[Migration] V{:03}_{} applied successfully", version, name);
        }
    }

    Ok(())
}

/// 执行单个迁移文件
async fn execute_migration(db: &Database, version: i32, name: &str, sql: &str) -> Result<()> {
    // 按 "-- @statement" 分隔符拆分为独立语句并逐条执行
    for statement in sql.split("-- @statement") {
        let stmt = statement.trim();
        if stmt.is_empty() || stmt.starts_with("--") && !stmt.contains('\n') {
            continue;
        }
        // 跳过纯注释块
        let effective: String = stmt.lines()
            .filter(|line| !line.trim_start().starts_with("--"))
            .collect::<Vec<_>>()
            .join("\n");
        let effective = effective.trim();
        if effective.is_empty() {
            continue;
        }
        sqlx::query(stmt).execute(&db.pool).await
            .map_err(|e| anyhow::anyhow!("Migration V{:03}_{} failed on statement:\n{}\nError: {}", version, name, &stmt[..stmt.len().min(200)], e))?;
    }

    // 记录已执行的版本
    sqlx::query(
        "INSERT INTO _schema_version (version, name, applied_at) VALUES (?, ?, UNIX_TIMESTAMP()*1000)"
    )
    .bind(version)
    .bind(name)
    .execute(&db.pool).await?;

    Ok(())
}
