//! Quota system module — rule-based multi-dimension quota enforcement

pub mod types;
pub mod counter;
pub mod checker;
pub mod sqlite_counter;
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
pub mod mysql_counter;
#[cfg(feature = "redis")]
pub mod redis_counter;

pub use types::{QuotaRule, QuotaCheckResult, QuotaContext, UsageEntry};
pub use counter::QuotaCounter;
pub use checker::QuotaChecker;
pub use sqlite_counter::SqliteQuotaCounter;
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
pub use mysql_counter::MysqlQuotaCounter;
#[cfg(feature = "redis")]
pub use redis_counter::RedisQuotaCounter;
