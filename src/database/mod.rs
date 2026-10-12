// 数据库模块 — cfg 条件分发

pub mod dao;
pub mod cleanup;
pub mod monitor;

#[cfg(feature = "sqlite")]
pub mod sqlite;
#[cfg(feature = "sqlite")]
pub use sqlite::Database;
#[cfg(feature = "sqlite")]
pub use sqlite::{init, init_with_config};

#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
pub mod mysql;
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
pub use mysql::Database;
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
pub use mysql::{init, init_with_config};

// Re-export convenience types (available regardless of backend)
pub use cleanup::DatabaseCleaner;
#[allow(unused_imports)]
pub use monitor::DatabaseMonitor;
