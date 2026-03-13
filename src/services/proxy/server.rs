use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::any,
    Router,
};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use crate::database::Database;
use crate::models::Provider;
use crate::database::dao::ProviderDao;
use super::circuit_breaker::CircuitBreaker;
use super::handlers::proxy_handler;

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

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .connect_timeout(std::time::Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        let state = ProxyState {
            db,
            current_provider: Arc::new(RwLock::new(None)),
            failover_queue: Arc::new(RwLock::new(Vec::new())),
            request_count: Arc::new(RwLock::new(0)),
            http_client,
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config),
        };

        Self { state, bind, port }
    }

    /// Start the proxy server
    pub async fn start(self) -> Result<()> {
        self.initialize_providers().await?;

        // Database cleanup if enabled
        if self.state.config.database.auto_cleanup {
            self.check_and_cleanup_database().await?;
        }

        let app = Router::new()
            .route("/v1/*path", any(proxy_handler))
            .route("/health", any(health_handler))
            .route("/status", any(status_handler))
            .with_state(self.state);

        let addr = format!("{}:{}", self.bind, self.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        tracing::info!("Proxy server listening on {}", addr);

        axum::serve(listener, app).await?;

        Ok(())
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
        let all_providers = ProviderDao::get_all_providers(&self.state.db)?;

        let active_providers: Vec<Provider> = all_providers
            .into_iter()
            .filter(|p| p.is_active)
            .collect();

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
