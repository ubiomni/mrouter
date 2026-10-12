// Admin API server

use anyhow::Result;
use axum::{
    middleware,
    routing::{get, post, put, delete},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{CorsLayer, Any};
use crate::config::AppConfig;
use crate::database::Database;
use crate::services::proxy::server::ProxyState;
use super::handlers;
use super::handlers_models;
use super::handlers_search;
use super::handlers_settings;
use super::middleware::{jwt_auth_middleware, JwtSecret};
use super::auth::get_jwt_secret;

/// Admin API state (shared by admin handler)
#[derive(Clone)]
pub struct AdminState {
    pub db: Database,
    pub config: Arc<AppConfig>,
    pub proxy_state: ProxyState,
    pub jwt_secret: String,
}

/// Admin API server
pub struct AdminServer {
    state: AdminState,
    bind: String,
    port: u16,
}

impl AdminServer {
    pub fn new(db: Database, config: Arc<AppConfig>, proxy_state: ProxyState) -> Self {
        let bind = config.admin.bind.clone();
        let port = config.admin.port;
        let jwt_secret = get_jwt_secret(config.admin.jwt_secret.as_deref());

        let state = AdminState {
            db,
            config: config.clone(),
            proxy_state,
            jwt_secret,
        };

        Self { state, bind, port }
    }

    pub async fn start(self) -> Result<()> {
        let addr = format!("{}:{}", self.bind, self.port);
        let app = self.build_router();
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        tracing::info!("Admin API server listening on {}", addr);

        axum::serve(listener, app).await?;
        Ok(())
    }

    fn build_router(self) -> Router {
        let jwt_secret = self.state.jwt_secret.clone();
        let state = self.state;
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        // Public routes (no auth required)
        let public = Router::new()
            .route("/api/auth/login", post(handlers::login))
            .route("/api/health", get(handlers::health_check));

        // Protected routes (JWT required)
        let protected = Router::new()
            // Auth
            .route("/api/auth/me", get(handlers::auth_me))
            // Organizations
            .route("/api/organizations", get(handlers::list_organizations).post(handlers::create_organization))
            .route("/api/organizations/{id}", put(handlers::update_organization))
            // Teams
            // Users
            .route("/api/users", get(handlers::list_users).post(handlers::create_user))
            .route("/api/users/{id}", put(handlers::update_user).delete(handlers::delete_user))
            // API Keys
            .route("/api/keys", get(handlers::list_keys).post(handlers::create_key))
            .route("/api/keys/{id}", put(handlers::update_key).delete(handlers::delete_key))
            // Providers
            .route("/api/providers", get(handlers::list_providers).post(handlers::create_provider))
            .route("/api/providers/export", get(handlers::export_providers))
            .route("/api/providers/import", post(handlers::import_providers))
            .route("/api/providers/{id}", get(handlers::get_provider).put(handlers::update_provider).delete(handlers::delete_provider))
            .route("/api/providers/{id}/activate", post(handlers::activate_provider))
            .route("/api/providers/{id}/deactivate", post(handlers::deactivate_provider))
            .route("/api/providers/{id}/test", post(handlers::test_provider))
            .route("/api/providers/{id}/fetch-models", get(handlers::fetch_provider_models))
            .route("/api/providers/fetch-models", post(handlers::fetch_models_adhoc))
            // Tokens (legacy)
            .route("/api/tokens", get(handlers::list_tokens).post(handlers::create_token))
            .route("/api/tokens/stats", get(handlers::token_stats_all))
            .route("/api/tokens/{id}", get(handlers::get_token).put(handlers::update_token).delete(handlers::delete_token))
            .route("/api/tokens/{id}/regenerate", post(handlers::regenerate_token))
            .route("/api/tokens/{id}/stats", get(handlers::token_stats_one))
            // Logs
            .route("/api/logs", get(handlers::list_logs))
            // Stats
            .route("/api/stats/summary", get(handlers::stats_summary))
            // Config
            .route("/api/config", get(handlers::get_config).put(handlers::update_config))
            // Proxy status
            .route("/api/proxy/status", get(handlers::proxy_status))
            .route("/api/proxy/circuit-breaker/{id}/reset", post(handlers::reset_circuit_breaker))
            // Quota (rule-based engine)
            .route("/api/quota/rules", get(handlers::list_quota_rules).post(handlers::create_quota_rule))
            .route("/api/quota/rules/{id}", put(handlers::update_quota_rule).delete(handlers::delete_quota_rule))
            .route("/api/quota/usage", get(handlers::get_quota_usage))
            .route("/api/quota/groups", get(handlers::get_quota_groups))
            .route("/api/quota/cache/invalidate", post(handlers::invalidate_quota_cache))
            // Provider Models
            .route("/api/providers/{id}/models", get(handlers_models::list_provider_models).post(handlers_models::create_provider_model))
            .route("/api/provider-models/available", get(handlers_models::list_available_models))
            .route("/api/provider-models/{id}", put(handlers_models::update_provider_model).delete(handlers_models::delete_provider_model))
            // Provider Endpoints
            .route("/api/providers/{id}/endpoints", get(handlers::list_provider_endpoints).post(handlers::create_provider_endpoint))
            .route("/api/provider-endpoints/{id}", put(handlers::update_provider_endpoint).delete(handlers::delete_provider_endpoint))
            // Data Migration
            .route("/api/migration/export", get(handlers::export_migration))
            .route("/api/migration/import", post(handlers::import_migration))
            // Model Metadata
            .route("/api/models/metadata", get(handlers_models::list_models_metadata).post(handlers_models::upsert_model_metadata))
            .route("/api/models/metadata/{model_name}", delete(handlers_models::delete_model_metadata))
            // System Settings
            .route("/api/settings", get(handlers_settings::list_settings))
            .route("/api/settings/{key}", get(handlers_settings::get_setting).put(handlers_settings::upsert_setting).delete(handlers_settings::delete_setting))
            // Provider Type Presets
            .route("/api/provider-presets", get(handlers::list_provider_presets))
            .route("/api/provider-presets/{type}", get(handlers::get_provider_preset).put(handlers::upsert_provider_preset).delete(handlers::delete_provider_preset))
            // Search Gateway
            .route("/api/search/providers", get(handlers_search::list_search_providers))
            .route("/api/search/test", post(handlers_search::test_search))
            // Apply JWT middleware (Extension must be outer layer so it runs first)
            .layer(middleware::from_fn(jwt_auth_middleware))
            .layer(axum::Extension(JwtSecret(jwt_secret)));

        // Merge public and protected, apply CORS to all
        public
            .merge(protected)
            .layer(cors)
            .with_state(state)
    }
}
