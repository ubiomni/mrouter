// MySQL 数据库后端

use anyhow::Result;
use sqlx::mysql::MySqlPoolOptions;
use sqlx::MySqlPool;

pub mod migrations;

/// MySQL 数据库连接池
#[derive(Clone)]
pub struct Database {
    pub pool: MySqlPool,
}

impl Database {
    pub async fn new(url: &str, max_connections: u32) -> Result<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(max_connections)
            .connect(url)
            .await?;
        Ok(Self { pool })
    }
}

/// 使用配置初始化 MySQL 数据库
pub async fn init_with_config(config: &crate::config::AppConfig) -> Result<Database> {
    let url = &config.database.path;
    let max_conn = config.database.pool_max_connections;

    let db = Database::new(url, max_conn).await?;

    // 运行迁移
    migrations::run_migrations(&db).await?;

    Ok(db)
}

/// 使用默认路径初始化数据库 (CLI 命令用)
pub async fn init() -> Result<Database> {
    let config = crate::config::AppConfig::load()?;
    init_with_config(&config).await
}
