//! /v1/models endpoint — OpenAI-compatible models list (data plane)

use axum::{
    extract::State,
    response::IntoResponse,
    http::StatusCode,
    Json,
};
use serde_json::json;
use crate::database::dao::{ProviderModelDao, ModelMetadataDao, ProviderDao};
use super::server::ProxyState;

/// GET /v1/models — returns aggregated models from provider_models table,
/// enriched with metadata from models_metadata.
///
/// Logic:
/// 1. Get all distinct model_patterns from provider_models (enabled only)
/// 2. Filter out wildcard patterns (entries containing "*")
/// 3. LEFT JOIN with models_metadata for display info
/// 4. Return as OpenAI-compatible list
pub async fn list_available_models(
    State(state): State<ProxyState>,
) -> impl IntoResponse {
    // Gather all active providers
    let providers = match ProviderDao::get_all_active(&state.db, 1) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": { "message": e.to_string(), "type": "internal_error" } })),
            );
        }
    };

    // Collect all distinct model patterns from enabled provider_models
    let mut model_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Track which provider_type owns each model for "owned_by" field
    let mut model_owner: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for provider in &providers {
        if let Ok(models) = ProviderModelDao::list_active_by_provider(&state.db, provider.id) {
            for m in models {
                // Skip wildcard patterns
                if m.model_pattern.contains('*') {
                    continue;
                }
                model_owner.entry(m.model_pattern.clone())
                    .or_insert_with(|| provider.name.to_string());
                model_names.insert(m.model_pattern);
            }
        }
    }

    // Load all model metadata for enrichment
    let metadata_map: std::collections::HashMap<String, crate::models::ModelMetadata> =
        ModelMetadataDao::list_all(&state.db)
            .unwrap_or_default()
            .into_iter()
            .map(|m| (m.model_name.clone(), m))
            .collect();

    // Build OpenAI-compatible response
    let mut data: Vec<serde_json::Value> = model_names.iter().map(|name| {
        let owned_by = model_owner.get(name)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let mut entry = json!({
            "id": name,
            "object": "model",
            "created": 1700000000i64,
            "owned_by": owned_by,
        });

        // Enrich with metadata if available
        if let Some(meta) = metadata_map.get(name) {
            if let Some(ref display_name) = meta.display_name {
                entry["display_name"] = json!(display_name);
            }
            if let Some(ref vendor) = meta.vendor {
                entry["vendor"] = json!(vendor);
            }
            if let Some(ref model_group) = meta.model_group {
                entry["model_group"] = json!(model_group);
            }
            if let Some(ref tags) = meta.tags {
                entry["tags"] = json!(tags);
            }
            if !meta.capabilities.is_empty() {
                entry["capabilities"] = json!(meta.capabilities);
            }
        }

        entry
    }).collect();

    // Sort by model id for deterministic output
    data.sort_by(|a, b| {
        let a_id = a["id"].as_str().unwrap_or("");
        let b_id = b["id"].as_str().unwrap_or("");
        a_id.cmp(b_id)
    });

    (
        StatusCode::OK,
        Json(json!({
            "object": "list",
            "data": data
        })),
    )
}
