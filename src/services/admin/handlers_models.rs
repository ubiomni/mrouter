// Admin API handlers — Provider Models & Model Metadata management

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::database::dao::{ProviderModelDao, ModelMetadataDao, ProviderDao};
use crate::models::{ProviderModel, ModelMetadata};
use super::server::AdminState;
use super::handlers::{ok, ok_void, err};

// Response DTO for model metadata (converts tags string to array for frontend)
#[derive(Serialize)]
pub struct ModelMetadataResponse {
    pub model_name: String,
    pub display_name: Option<String>,
    pub vendor: Option<String>,
    pub tags: Vec<String>,
    pub capabilities: Vec<String>,
    pub is_visible: bool,
}

impl From<ModelMetadata> for ModelMetadataResponse {
    fn from(meta: ModelMetadata) -> Self {
        let tags = meta.tags
            .as_ref()
            .map(|s| s.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
            .unwrap_or_default();

        Self {
            model_name: meta.model_name,
            display_name: meta.display_name,
            vendor: meta.vendor,
            tags,
            capabilities: meta.capabilities,
            is_visible: meta.is_visible,
        }
    }
}

// ============================================================
//  Provider Models
// ============================================================

#[derive(Deserialize)]
pub struct CreateProviderModelRequest {
    pub model_pattern: String,
    pub upstream_model: Option<String>,
    pub priority: Option<i32>,
    pub weight: Option<i32>,
    pub enabled: Option<bool>,
}

#[derive(Deserialize)]
pub struct UpdateProviderModelRequest {
    pub model_pattern: Option<String>,
    pub upstream_model: Option<String>,
    pub priority: Option<i32>,
    pub weight: Option<i32>,
    pub enabled: Option<bool>,
}

/// GET /api/providers/:id/models — list model associations for a provider
pub async fn list_provider_models(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match ProviderModelDao::list_by_provider(&state.db, id) {
        Ok(models) => ok(models).into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

/// POST /api/providers/:id/models — add model association
pub async fn create_provider_model(
    State(state): State<AdminState>,
    Path(provider_id): Path<i64>,
    Json(body): Json<CreateProviderModelRequest>,
) -> impl IntoResponse {
    // Default org_id = 1 for now (multi-org can be extracted from JWT later)
    let mut model = ProviderModel::new(1, provider_id, body.model_pattern);
    model.upstream_model = body.upstream_model;
    model.priority = body.priority.unwrap_or(0);
    model.weight = body.weight.unwrap_or(0);
    model.enabled = body.enabled.unwrap_or(true);

    match ProviderModelDao::insert(&state.db, &model) {
        Ok(id) => {
            tracing::info!("Provider model created: id={}, provider_id={}", id, provider_id);
            // Invalidate Redis cache key (all instances see fresh data on next read)
            state.proxy_state.model_router_cache.invalidate_provider(provider_id);
            match ProviderModelDao::get_by_id(&state.db, id) {
                Ok(Some(m)) => ok(m).into_response(),
                _ => ok(json!({ "id": id })).into_response(),
            }
        }
        Err(e) => err(400, &e.to_string()).into_response(),
    }
}

/// PUT /api/provider-models/:id — update model association
pub async fn update_provider_model(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateProviderModelRequest>,
) -> impl IntoResponse {
    match ProviderModelDao::get_by_id(&state.db, id) {
        Ok(Some(mut model)) => {
            if let Some(pattern) = body.model_pattern {
                model.model_pattern = pattern;
            }
            if let Some(upstream) = body.upstream_model {
                model.upstream_model = if upstream.is_empty() { None } else { Some(upstream) };
            }
            if let Some(p) = body.priority {
                model.priority = p;
            }
            if let Some(w) = body.weight {
                model.weight = w;
            }
            if let Some(e) = body.enabled {
                model.enabled = e;
            }

            match ProviderModelDao::update(&state.db, &model) {
                Ok(_) => {
                    tracing::info!("Provider model updated: id={}", id);
                    // Invalidate Redis cache key
                    state.proxy_state.model_router_cache.invalidate_provider(model.provider_id);
                    ok(model).into_response()
                }
                Err(e) => err(500, &e.to_string()).into_response(),
            }
        }
        Ok(None) => err(404, "Provider model not found").into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

/// DELETE /api/provider-models/:id — delete model association
pub async fn delete_provider_model(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    // Get provider_id before deleting (for cache invalidation)
    let provider_id = ProviderModelDao::get_by_id(&state.db, id)
        .ok()
        .flatten()
        .map(|m| m.provider_id);

    match ProviderModelDao::delete(&state.db, id) {
        Ok(_) => {
            tracing::info!("Provider model deleted: id={}", id);
            // Invalidate Redis cache key
            if let Some(pid) = provider_id {
                state.proxy_state.model_router_cache.invalidate_provider(pid);
            }
            ok_void().into_response()
        }
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

// ============================================================
//  Model Metadata
// ============================================================

#[derive(Deserialize)]
pub struct UpsertModelMetadataRequest {
    pub model_name: String,
    pub display_name: Option<String>,
    pub vendor: Option<String>,
    pub model_group: Option<String>,
    pub icon: Option<String>,
    pub tags: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub is_visible: Option<bool>,
    pub sort_order: Option<i32>,
}

/// GET /api/models/metadata — list all model metadata
pub async fn list_models_metadata(
    State(state): State<AdminState>,
) -> impl IntoResponse {
    match ModelMetadataDao::list_all(&state.db) {
        Ok(metas) => {
            let response: Vec<ModelMetadataResponse> = metas.into_iter().map(|m| m.into()).collect();
            ok(response).into_response()
        },
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

/// POST /api/models/metadata — create/update model metadata (upsert)
pub async fn upsert_model_metadata(
    State(state): State<AdminState>,
    Json(body): Json<UpsertModelMetadataRequest>,
) -> impl IntoResponse {
    // Default org_id = 1
    let meta = ModelMetadata {
        org_id: 1,
        model_name: body.model_name.clone(),
        display_name: body.display_name,
        vendor: body.vendor,
        model_group: body.model_group,
        icon: body.icon,
        tags: body.tags,
        capabilities: body.capabilities.unwrap_or_default(),
        is_visible: body.is_visible.unwrap_or(true),
        sort_order: body.sort_order.unwrap_or(0),
    };

    match ModelMetadataDao::upsert(&state.db, &meta) {
        Ok(_) => {
            match ModelMetadataDao::get(&state.db, 1, &body.model_name) {
                Ok(Some(m)) => ok(m).into_response(),
                _ => ok_void().into_response(),
            }
        }
        Err(e) => err(400, &e.to_string()).into_response(),
    }
}

/// DELETE /api/models/metadata/:model_name — delete metadata
pub async fn delete_model_metadata(
    State(state): State<AdminState>,
    Path(model_name): Path<String>,
) -> impl IntoResponse {
    match ModelMetadataDao::delete(&state.db, 1, &model_name) {
        Ok(_) => ok_void().into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

/// GET /api/provider-models/available — list all model patterns with their provider_group
/// Returns: [{ model: "gpt-4o", provider_group: "default" }, ...]
/// Used by Keys page to populate model_limits dropdown with group-aware filtering.
pub async fn list_available_models(
    State(state): State<AdminState>,
) -> impl IntoResponse {
    // Build provider_id -> provider_group map
    let providers = match ProviderDao::get_all(&state.db, 0) {
        Ok(p) => p,
        Err(e) => return err(500, &e.to_string()).into_response(),
    };
    let group_map: std::collections::HashMap<i64, String> = providers
        .iter()
        .map(|p| (p.id, p.provider_group.clone()))
        .collect();

    // Fetch all enabled provider_models (org_id=1 for now)
    let models = match ProviderModelDao::list_by_org(&state.db, 1) {
        Ok(m) => m,
        Err(e) => return err(500, &e.to_string()).into_response(),
    };

    // Deduplicate: collect unique (model_pattern, provider_group) pairs
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for m in &models {
        let group = group_map.get(&m.provider_id).cloned().unwrap_or_default();
        let key = (m.model_pattern.clone(), group.clone());
        if seen.insert(key) {
            result.push(json!({
                "model": m.model_pattern,
                "provider_group": group,
            }));
        }
    }

    ok(result).into_response()
}
