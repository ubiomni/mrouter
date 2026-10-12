//! Unified Redis connection abstraction supporting both single-node and cluster mode.
//!
//! Provides `RedisPool` — create once at startup, clone cheaply, pass to all components.
//! All consumers use `pool.get_conn()` to get a `RedisConn` that works identically
//! regardless of the underlying mode.

#[cfg(feature = "redis")]
use redis::aio::ConnectionLike;
#[cfg(feature = "redis")]
use redis::{Cmd, Pipeline, RedisFuture, RedisResult, Value};

use crate::config::RedisConfig;

/// Unified async Redis connection that works for both single-node and cluster.
///
/// Wraps either a `MultiplexedConnection` or a `ClusterConnection`.
/// Both implement `redis::aio::ConnectionLike`, so all `redis::cmd()...query_async()`
/// and `redis::Script...invoke_async()` calls work transparently.
#[cfg(feature = "redis")]
#[derive(Clone)]
pub enum RedisConn {
    Single(redis::aio::MultiplexedConnection),
    Cluster(redis::cluster_async::ClusterConnection),
}

#[cfg(feature = "redis")]
impl ConnectionLike for RedisConn {
    fn req_packed_command<'a>(&'a mut self, cmd: &'a Cmd) -> RedisFuture<'a, Value> {
        match self {
            RedisConn::Single(c) => c.req_packed_command(cmd),
            RedisConn::Cluster(c) => c.req_packed_command(cmd),
        }
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        cmd: &'a Pipeline,
        offset: usize,
        count: usize,
    ) -> RedisFuture<'a, Vec<Value>> {
        match self {
            RedisConn::Single(c) => c.req_packed_commands(cmd, offset, count),
            RedisConn::Cluster(c) => c.req_packed_commands(cmd, offset, count),
        }
    }

    fn get_db(&self) -> i64 {
        match self {
            RedisConn::Single(c) => c.get_db(),
            RedisConn::Cluster(c) => c.get_db(),
        }
    }
}

/// Redis connection pool — cheaply cloneable handle.
///
/// Create once with `RedisPool::new()`, then clone and pass to components.
/// Each `get_conn()` call returns a connection ready for commands.
#[cfg(feature = "redis")]
#[derive(Clone)]
pub enum RedisPool {
    /// Single-node mode: uses a multiplexed connection via `redis::Client`.
    Single(redis::Client),
    /// Cluster mode: uses `redis::cluster::ClusterClient`.
    Cluster(redis::cluster::ClusterClient),
}

#[cfg(feature = "redis")]
impl RedisPool {
    /// Create a pool from config. Returns None if connection setup fails.
    pub fn new(config: &RedisConfig) -> Option<Self> {
        if config.cluster {
            Self::new_cluster(config)
        } else {
            Self::new_single(config)
        }
    }

    fn new_single(config: &RedisConfig) -> Option<Self> {
        let url = config.connection_url();
        match redis::Client::open(url.as_str()) {
            Ok(client) => {
                tracing::info!("[Redis] Single-node client created: {}", Self::redact_url(&url));
                Some(RedisPool::Single(client))
            }
            Err(e) => {
                tracing::error!("[Redis] Failed to create single-node client: {}", e);
                None
            }
        }
    }

    fn new_cluster(config: &RedisConfig) -> Option<Self> {
        let url = config.connection_url();
        // ClusterClient accepts a list of initial nodes; one seed node is enough
        // — the client discovers the full topology via CLUSTER SLOTS.
        let mut builder = redis::cluster::ClusterClientBuilder::new(vec![url.as_str()]);
        if let Some(ref pw) = config.password {
            builder = builder.password(pw.clone());
        }
        match builder.build() {
            Ok(client) => {
                tracing::info!("[Redis] Cluster client created: {}", Self::redact_url(&url));
                Some(RedisPool::Cluster(client))
            }
            Err(e) => {
                tracing::error!("[Redis] Failed to create cluster client: {}", e);
                None
            }
        }
    }

    /// Get an async connection. Cheap for multiplexed; creates a new logical
    /// connection for cluster (internally pooled by the cluster client).
    pub async fn get_conn(&self) -> RedisResult<RedisConn> {
        match self {
            RedisPool::Single(client) => {
                let conn = client.get_multiplexed_async_connection().await?;
                Ok(RedisConn::Single(conn))
            }
            RedisPool::Cluster(client) => {
                let conn = client.get_async_connection().await?;
                Ok(RedisConn::Cluster(conn))
            }
        }
    }

    /// Get a PubSub connection (single-node only).
    /// Cluster mode pub/sub uses shard channels via the regular connection.
    pub async fn get_pubsub(&self) -> RedisResult<redis::aio::PubSub> {
        match self {
            RedisPool::Single(client) => client.get_async_pubsub().await,
            RedisPool::Cluster(_) => {
                // For cluster pub/sub, we need a single-node connection to one of the nodes.
                // The ClusterClient doesn't expose PubSub directly.
                // Workaround: create a regular client to the same seed node.
                Err(redis::RedisError::from((
                    redis::ErrorKind::ClientError,
                    "PubSub not directly available in cluster mode; use ClusterConnection.subscribe()",
                )))
            }
        }
    }

    /// Whether this pool is in cluster mode.
    pub fn is_cluster(&self) -> bool {
        matches!(self, RedisPool::Cluster(_))
    }

    /// Redact password from URL for logging.
    fn redact_url(url: &str) -> String {
        if let Some(at_pos) = url.find('@') {
            if let Some(colon_pos) = url[..at_pos].rfind(':') {
                return format!("{}:***@{}", &url[..colon_pos], &url[at_pos + 1..]);
            }
        }
        url.to_string()
    }
}
