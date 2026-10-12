/// Cache invalidation via Redis pub/sub for multi-instance deployments.
///
/// When Redis is configured (feature "distributed"), mutations trigger publish
/// to a Redis channel; all gateway instances subscribe and invalidate local caches.
/// Without Redis, falls back to local-only broadcast (single instance mode).

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// The Redis pub/sub channel name for cache invalidation
const INVALIDATION_CHANNEL: &str = "mrouter:gateway:invalidate";

/// Events that require cache invalidation across instances
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InvalidationEvent {
    /// A provider's configuration was updated
    ProviderUpdated { provider_id: i64 },
    /// A provider's model list was updated
    ProviderModelUpdated { provider_id: i64 },
    /// An API key was created, updated, or revoked
    ApiKeyUpdated { key_id: i64 },
    /// A settings value was changed
    SettingsUpdated { key: String },
    /// The route table needs a full rebuild
    RouteTableRebuild,
    /// Provider was auto-banned by another instance
    ProviderBanned { provider_id: i64 },
    /// Provider was re-enabled by another instance
    ProviderEnabled { provider_id: i64 },
    /// Circuit breaker tripped on another instance
    CircuitBreakerTripped { provider_id: i64 },
    /// Circuit breaker recovered on another instance
    CircuitBreakerRecovered { provider_id: i64 },
}

/// Cache invalidation manager.
///
/// Always provides local broadcast. When compiled with `distributed` feature
/// and a Redis URL is configured, also publishes/subscribes via Redis pub/sub.
pub struct CacheInvalidator {
    /// Local broadcast channel (always functional)
    local_tx: broadcast::Sender<InvalidationEvent>,
    /// Redis invalidator (only present when feature enabled AND url configured)
    #[cfg(feature = "redis")]
    redis: Option<RedisInvalidator>,
}

#[cfg(feature = "redis")]
struct RedisInvalidator {
    client: redis::Client,
}

impl CacheInvalidator {
    /// Create a new CacheInvalidator.
    ///
    /// - Without `distributed` feature: always local-only.
    /// - With `distributed` feature: uses Redis if `redis_url` is Some.
    pub fn new(redis_url: Option<&str>) -> Self {
        let (local_tx, _) = broadcast::channel(256);

        #[cfg(feature = "redis")]
        let redis = redis_url.and_then(|url| {
            match redis::Client::open(url) {
                Ok(client) => {
                    tracing::info!("[CacheInvalidator] Redis connected: {}", url);
                    Some(RedisInvalidator { client })
                }
                Err(e) => {
                    tracing::error!("[CacheInvalidator] Failed to connect to Redis: {}", e);
                    None
                }
            }
        });

        #[cfg(not(feature = "redis"))]
        let _ = redis_url; // suppress unused warning

        Self {
            local_tx,
            #[cfg(feature = "redis")]
            redis,
        }
    }

    /// Publish an invalidation event.
    ///
    /// Sends to local broadcast subscribers and, if Redis is configured,
    /// also publishes to the Redis channel.
    pub async fn publish(&self, event: InvalidationEvent) {
        // Always broadcast locally
        let _ = self.local_tx.send(event.clone());

        #[cfg(feature = "redis")]
        if let Some(ref redis) = self.redis {
            let payload = match serde_json::to_string(&event) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("[CacheInvalidator] Failed to serialize event: {}", e);
                    return;
                }
            };

            match redis.client.get_multiplexed_async_connection().await {
                Ok(mut conn) => {
                    let result: Result<(), redis::RedisError> =
                        redis::cmd("PUBLISH")
                            .arg(INVALIDATION_CHANNEL)
                            .arg(&payload)
                            .query_async(&mut conn)
                            .await;
                    if let Err(e) = result {
                        tracing::error!("[CacheInvalidator] Redis PUBLISH failed: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("[CacheInvalidator] Redis connection failed: {}", e);
                }
            }
        }
    }

    /// Subscribe to invalidation events (local broadcast).
    ///
    /// Returns a receiver that gets events from both local publishes
    /// and (if Redis is configured) events received from Redis pub/sub.
    pub fn subscribe(&self) -> broadcast::Receiver<InvalidationEvent> {
        self.local_tx.subscribe()
    }

    /// Start the background Redis pub/sub listener.
    ///
    /// This subscribes to the Redis channel and re-broadcasts received events
    /// into the local broadcast channel so all local subscribers get them.
    /// No-op if Redis is not configured.
    pub async fn start_listener(self: &Arc<Self>) {
        #[cfg(feature = "redis")]
        {
            if let Some(ref redis) = self.redis {
                let client = redis.client.clone();
                let local_tx = self.local_tx.clone();

                tokio::spawn(async move {
                    Self::redis_listener_loop(client, local_tx).await;
                });
            }
        }

        #[cfg(not(feature = "redis"))]
        {
            tracing::debug!("[CacheInvalidator] No Redis configured, local-only mode");
        }
    }

    #[cfg(feature = "redis")]
    async fn redis_listener_loop(
        client: redis::Client,
        local_tx: broadcast::Sender<InvalidationEvent>,
    ) {
        use futures::StreamExt;

        loop {
            tracing::info!("[CacheInvalidator] Connecting to Redis pub/sub...");

            let pubsub_conn = match client.get_async_pubsub().await {
                Ok(conn) => conn,
                Err(e) => {
                    tracing::error!("[CacheInvalidator] Redis pub/sub connection failed: {}, retrying in 5s", e);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            };

            let mut pubsub = pubsub_conn;
            if let Err(e) = pubsub.subscribe(INVALIDATION_CHANNEL).await {
                tracing::error!("[CacheInvalidator] Redis SUBSCRIBE failed: {}, retrying in 5s", e);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }

            tracing::info!("[CacheInvalidator] Subscribed to Redis channel: {}", INVALIDATION_CHANNEL);

            let mut msg_stream = pubsub.on_message();
            while let Some(msg) = msg_stream.next().await {
                let payload: String = match msg.get_payload() {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!("[CacheInvalidator] Invalid message payload: {}", e);
                        continue;
                    }
                };

                match serde_json::from_str::<InvalidationEvent>(&payload) {
                    Ok(event) => {
                        tracing::debug!("[CacheInvalidator] Received event from Redis: {:?}", event);
                        let _ = local_tx.send(event);
                    }
                    Err(e) => {
                        tracing::warn!("[CacheInvalidator] Failed to deserialize event: {}", e);
                    }
                }
            }

            tracing::warn!("[CacheInvalidator] Redis pub/sub stream ended, reconnecting in 2s");
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }
}
