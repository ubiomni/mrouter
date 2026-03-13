// 数据库模块

use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub mod dao;
pub mod migrations;
pub mod cleanup;
pub mod monitor;

pub use cleanup::{DatabaseCleaner, CleanupStats};
pub use monitor::{DatabaseMonitor, DbPerformanceMetrics, QueryPerformance};

/// 数据库连接
#[derive(Clone)]
pub struct Database {
    pub conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(path: PathBuf) -> Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn execute<P>(&self, sql: &str, params: P) -> Result<usize>
    where
        P: rusqlite::Params,
    {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute(sql, params)?)
    }

    pub fn query_row<T, P, F>(&self, sql: &str, params: P, f: F) -> Result<T>
    where
        P: rusqlite::Params,
        F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row(sql, params, f)?)
    }
}

/// 使用配置初始化数据库
pub async fn init_with_config(config: &crate::config::AppConfig) -> Result<Database> {
    let db_path = config.resolve_db_path()?;

    // 确保目录存在
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let db = Database::new(db_path)?;

    // 启用 WAL 模式
    if config.database.wal_mode {
        let conn = db.conn.lock().unwrap();
        conn.pragma_update(None, "journal_mode", "WAL")?;
    }

    // 运行迁移
    migrations::run_migrations(&db)?;

    Ok(db)
}

/// 使用默认路径初始化数据库 (CLI 命令用)
pub async fn init() -> Result<Database> {
    let config = crate::config::AppConfig::load()?;
    init_with_config(&config).await
}
