use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{any, get},
    Router,
};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use crate::database::Database;
use crate::models::Provider;
use crate::database::dao::ProviderDao;
use super::auth::ApiKeyAuthenticator;
use super::circuit_breaker::CircuitBreaker;
use super::format_registry::FormatRegistry;
use super::handlers::proxy_handler;
use super::models_endpoint::list_available_models;
use super::realtime::realtime_handler;
use super::cache_invalidation::CacheInvalidator;
use super::distributed_state::DistributedState;
use super::provider_cache::ProviderCache;
use super::provider_limiter::ProviderRateLimiter;
use super::model_router::ModelRouterCache;
use crate::services::quota::QuotaChecker;

/// Proxy server state (shared across all request handlers)
#[derive(Clone)]
pub struct ProxyState {
    pub db: Database,
    pub current_provider: Arc<RwLock<Option<Provider>>>,
    pub failover_queue: Arc<RwLock<Vec<Provider>>>,
    pub request_count: Arc<RwLock<u64>>,
    pub http_client: reqwest::Client,
    pub circuit_breakers: Arc<RwLock<HashMap<i64, Arc<CircuitBreaker>>>>,
    pub config: Arc<crate::config::AppConfig>,
    pub format_registry: Arc<FormatRegistry>,
    pub invalidator: Arc<CacheInvalidator>,
    pub distributed: Arc<DistributedState>,
    pub provider_cache: Arc<ProviderCache>,
    pub authenticator: Arc<ApiKeyAuthenticator>,
    pub provider_limiter: Arc<ProviderRateLimiter>,
    pub quota_checker: Arc<QuotaChecker>,
    pub model_router_cache: Arc<ModelRouterCache>,
}

/// Proxy server
pub struct ProxyServer {
    state: ProxyState,
    bind: String,
    port: u16,
}

impl ProxyServer {
    pub fn new(db: Database, bind: String, port: u16) -> Self {
        let config = crate::config::AppConfig::load()
            .expect("Failed to load config");

        let mut client_builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .connect_timeout(std::time::Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .tcp_keepalive(std::time::Duration::from_secs(60));

        // Configure upstream proxy
        match config.proxy.upstream_proxy.as_deref() {
            Some("none") | Some("") => {
                // Explicitly disable system proxy
                client_builder = client_builder.no_proxy();
                tracing::info!("[Proxy] Upstream proxy disabled (no_proxy)");
            }
            Some(proxy_url) => {
                match reqwest::Proxy::all(proxy_url) {
                    Ok(proxy) => {
                        client_builder = client_builder.proxy(proxy);
                        tracing::info!("[Proxy] Using upstream proxy: {}", proxy_url);
                    }
                    Err(e) => {
                        tracing::error!("[Proxy] Invalid upstream_proxy '{}': {}, falling back to system proxy", proxy_url, e);
                    }
                }
            }
            None => {
                // Follow system env (http_proxy, https_proxy, ALL_PROXY)
                tracing::info!("[Proxy] Using system proxy settings (from env)");
            }
        }

        let http_client = client_builder.build()
            .expect("Failed to create HTTP client");

        // Initialize Redis pool (single-node or cluster, depending on config)
        #[cfg(feature = "redis")]
        let redis_pool: Option<Arc<crate::utils::redis::RedisPool>> = config.redis.as_ref()
            .and_then(|r| crate::utils::redis::RedisPool::new(r))
            .map(Arc::new);
        #[cfg(not(feature = "redis"))]
        let redis_pool: Option<()> = None;

        // Build a legacy redis_url for components not yet migrated (cache_invalidation PubSub)
        let redis_url_owned = config.redis.as_ref().map(|r| r.connection_url());
        let redis_url = redis_url_owned.as_deref();

        // Initialize cache invalidator (uses Redis if configured)
        let invalidator = Arc::new(CacheInvalidator::new(redis_url));

        // Initialize distributed state (uses Redis if configured)
        #[cfg(feature = "redis")]
        let distributed = Arc::new(DistributedState::new(
            redis_pool.clone(),
            invalidator.clone(),
            config.circuit_breaker.failure_threshold,
        ));
        #[cfg(not(feature = "redis"))]
        let distributed = Arc::new(DistributedState::new(
            invalidator.clone(),
            config.circuit_breaker.failure_threshold,
        ));

        // Initialize provider cache (uses Redis if configured)
        #[cfg(feature = "redis")]
        let provider_cache = Arc::new(ProviderCache::new(db.clone(), redis_pool.clone()));
        #[cfg(not(feature = "redis"))]
        let provider_cache = Arc::new(ProviderCache::new(db.clone()));

        // Initialize API key authenticator (60s in-memory cache, O(1) cache hit)
        let authenticator = Arc::new({
            #[cfg(feature = "redis")]
            {
                match redis_pool {
                    Some(ref pool) => ApiKeyAuthenticator::with_redis(db.clone(), pool.clone()),
                    None => ApiKeyAuthenticator::new(db.clone()),
                }
            }
            #[cfg(not(feature = "redis"))]
            {
                ApiKeyAuthenticator::new(db.clone())
            }
        });

        // Initialize provider-level rate limiter
        let provider_limiter = Arc::new({
            #[cfg(feature = "redis")]
            {
                match redis_pool {
                    Some(ref pool) => ProviderRateLimiter::with_redis(pool.clone()),
                    None => ProviderRateLimiter::new(),
                }
            }
            #[cfg(not(feature = "redis"))]
            {
                ProviderRateLimiter::new()
            }
        });

        // Initialize quota checker (uses Redis counter if configured, else DB-native counter)
        let quota_checker = Arc::new({
            let counter: Arc<dyn crate::services::quota::QuotaCounter> = {
                #[cfg(feature = "redis")]
                {
                    if let Some(ref pool) = redis_pool {
                        match crate::services::quota::RedisQuotaCounter::new_with_pool(pool.clone()) {
                            Ok(rc) => {
                                tracing::info!("[Quota] Using Redis counter backend");
                                Arc::new(rc) as Arc<dyn crate::services::quota::QuotaCounter>
                            }
                            Err(e) => {
                                tracing::warn!("[Quota] Redis counter init failed ({}), using DB fallback", e);
                                Self::create_db_counter(db.clone())
                            }
                        }
                    } else {
                        Self::create_db_counter(db.clone())
                    }
                }
                #[cfg(not(feature = "redis"))]
                {
                    Self::create_db_counter(db.clone())
                }
            };
            #[cfg(feature = "redis")]
            {
                match redis_url {
                    Some(url) => QuotaChecker::with_redis(db.clone(), counter, url),
                    None => QuotaChecker::new(db.clone(), counter),
                }
            }
            #[cfg(not(feature = "redis"))]
            {
                QuotaChecker::new(db.clone(), counter)
            }
        });

        // Initialize model router cache (Redis if configured, else direct DB)
        let model_router_cache = Arc::new(ModelRouterCache::new(db.clone(), redis_url));

        let state = ProxyState {
            db,
            current_provider: Arc::new(RwLock::new(None)),
            failover_queue: Arc::new(RwLock::new(Vec::new())),
            request_count: Arc::new(RwLock::new(0)),
            http_client,
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config),
            format_registry: Arc::new(FormatRegistry::new()),
            invalidator,
            distributed,
            provider_cache,
            authenticator,
            provider_limiter,
            quota_checker,
            model_router_cache,
        };

        Self { state, bind, port }
    }

    /// Get a clone of the proxy state
    pub fn state(&self) -> ProxyState {
        self.state.clone()
    }

    /// Start the proxy server
    pub async fn start(self) -> Result<()> {
        let (listener, app) = self.bind().await?;

        tracing::info!("Proxy server listening on {}", listener.local_addr()?);

        axum::serve(listener, app).await?;

        Ok(())
    }

    /// Bind the proxy server and return the listener + router (ready to serve).
    /// This allows callers to signal readiness after bind but before serve.
    pub async fn bind(self) -> Result<(tokio::net::TcpListener, axum::Router)> {
        self.initialize_providers().await?;

        // Database cleanup if enabled
        if self.state.config.database.auto_cleanup {
            self.check_and_cleanup_database().await?;
        }

        // Start cache invalidation listener (subscribes to Redis pub/sub if configured)
        self.state.invalidator.start_listener().await;

        // Start background task to handle invalidation events
        self.spawn_invalidation_handler();

        let search_state = crate::services::search::SearchState {
            db: self.state.db.clone(),
            authenticator: self.state.authenticator.clone(),
            http_client: self.state.http_client.clone(),
        };

        let app = Router::new()
            .route("/v1/realtime", get(realtime_handler))
            .route("/v1/models", get(list_available_models))
            .route("/v1/search", any(crate::services::search::search_handler).with_state(search_state.clone()))
            .route("/v1/search/{*path}", any(crate::services::search::search_handler).with_state(search_state))
            .route("/v1/{*path}", any(proxy_handler))
            .route("/health", any(health_handler))
            .route("/status", any(status_handler))
            .with_state(self.state);

        let addr = format!("{}:{}", self.bind, self.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        Ok((listener, app))
    }

    /// Check and execute database cleanup
    async fn check_and_cleanup_database(&self) -> Result<()> {
        use crate::database::DatabaseCleaner;

        let cleaner = DatabaseCleaner::new(&self.state.db);
        let max_logs = self.state.config.database.max_request_logs;

        if cleaner.needs_cleanup(max_logs)? {
            tracing::info!("[Cleanup] Database cleanup needed, starting cleanup...");

            let archive_dir = &self.state.config.database.archive_dir;
            let stats = cleaner.cleanup(max_logs, archive_dir)?;

            tracing::info!(
                "[Cleanup] Cleanup completed: archived={}, deleted={}, duration={}ms",
                stats.archived_count,
                stats.deleted_count,
                stats.duration_ms
            );

            if let Some(ref archive_file) = stats.archive_file {
                tracing::info!("[Cleanup] Archive saved to: {}", archive_file);
            }
        } else {
            tracing::debug!("[Cleanup] No cleanup needed");
        }

        Ok(())
    }

    /// Initialize providers from database
    async fn initialize_providers(&self) -> Result<()> {
        let active_providers = ProviderDao::get_all_active(&self.state.db, 1)?;

        if active_providers.is_empty() {
            tracing::warn!("No active providers found for proxy");
        } else {
            tracing::info!("Loaded {} active provider(s) for proxy", active_providers.len());
            for p in &active_providers {
                tracing::info!("  - {} (priority: {})", p.name, p.priority);
            }
        }

        *self.state.failover_queue.write().await = active_providers;

        Ok(())
    }

    /// Create the appropriate DB-native quota counter based on compile feature
    fn create_db_counter(db: Database) -> Arc<dyn crate::services::quota::QuotaCounter> {
        #[cfg(feature = "sqlite")]
        {
            use crate::services::quota::SqliteQuotaCounter;
            tracing::info!("[Quota] Using SQLite counter backend");
            Arc::new(SqliteQuotaCounter::new(db))
        }
        #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
        {
            use crate::services::quota::MysqlQuotaCounter;
            tracing::info!("[Quota] Using MySQL counter backend");
            Arc::new(MysqlQuotaCounter::new(db))
        }
        #[cfg(not(any(feature = "sqlite", feature = "mysql")))]
        {
            use crate::services::quota::SqliteQuotaCounter;
            Arc::new(SqliteQuotaCounter::new(db))
        }
    }

    /// Spawn background task to handle cache invalidation events
    fn spawn_invalidation_handler(&self) {
        let mut rx = self.state.invalidator.subscribe();
        let _db = self.state.db.clone();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        tracing::debug!("[CacheInvalidation] Event received: {:?}", event);
                        // All cache invalidation is now handled via direct Redis DEL
                        // at the point of mutation. Pub/sub events are reserved for
                        // circuit breaker and auto-ban state sync between instances.
                    }
                    Err(e) => {
                        tracing::warn!("[CacheInvalidation] Invalidation receiver error: {}", e);
                    }
                }
            }
        });
    }
}

/// Health check handler
async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Status query handler
async fn status_handler(State(state): State<ProxyState>) -> impl IntoResponse {
    let count = *state.request_count.read().await;
    let provider = state.current_provider.read().await;

    let provider_name = provider.as_ref()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "None".to_string());

    let status = serde_json::json!({
        "status": "running",
        "request_count": count,
        "current_provider": provider_name,
    });

    (StatusCode::OK, serde_json::to_string(&status).unwrap())
}
