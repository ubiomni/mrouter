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

// Re-export convenience types (available regardless of backend)
pub use cleanup::{DatabaseCleaner, CleanupStats};
pub use monitor::{DatabaseMonitor, DbPerformanceMetrics, QueryPerformance};
