// Admin API handlers

use std::collections::HashMap;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::database::dao::{ProviderDao, ApiKeyDao, SystemSettingsDao, UserDao, OrganizationDao, ProviderPresetDao};
use crate::database::Database;
use crate::models::{User, UserRole, ApiKey, Organization, ProviderPresetFull};
use super::server::AdminState;
use super::auth::{self, Claims};
use super::permission::{self, RoleLevel, ResourceScope};

// ============================================================
//  Response helpers
// ============================================================

pub(crate) fn ok<T: Serialize>(data: T) -> impl IntoResponse {
    Json(json!({
        "code": 0,
        "message": "ok",
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "data": serde_json::to_value(data).unwrap_or(Value::Null)
    }))
}

pub(crate) fn ok_void() -> impl IntoResponse {
    Json(json!({
        "code": 0,
        "message": "ok",
        "timestamp": chrono::Utc::now().timestamp_millis()
    }))
}

pub(crate) fn err(code: u16, msg: &str) -> impl IntoResponse {
    (
        StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(json!({
            "code": code,
            "message": msg,
            "timestamp": chrono::Utc::now().timestamp_millis()
        })),
    )
}

// ============================================================
//  Providers
// ============================================================

#[derive(Deserialize)]
pub struct ListProvidersQuery {
    pub page: Option<u32>,
    pub size: Option<u32>,
    pub app_type: Option<String>,
    pub category: Option<String>,
}

pub async fn list_providers(
    State(state): State<AdminState>,
    Query(q): Query<ListProvidersQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let claims = match request.extensions().get::<Claims>().cloned() {
        Some(c) => c,
        None => return err(401, "Unauthorized").into_response(),
    };
    let is_admin = permission::is_org_admin(&claims);
    let page = q.page.unwrap_or(1).max(1);
    let size = q.size.unwrap_or(20).min(100);

    match ProviderDao::get_all(&state.db, 0) {
        Ok(providers) => {
            // Filter by category if requested
            let providers: Vec<_> = if let Some(ref cat) = q.category {
                providers.into_iter().filter(|p| p.category == *cat).collect()
            } else {
                providers
            };

            let total = providers.len() as u32;
            let start = ((page - 1) * size) as usize;
            let elements: Vec<Value> = if is_admin {
                providers.into_iter().skip(start).take(size as usize)
                    .map(|p| serde_json::to_value(p.with_masked_key()).unwrap_or(json!({})))
                    .collect()
            } else {
                // Non-admin: return only safe fields (no api_key, base_url, custom_headers)
                providers.into_iter().skip(start).take(size as usize)
                    .map(|p| json!({
                        "id": p.id,
                        "name": p.name,
                        "display_name": p.display_name,
                        "category": p.category,
                        "provider_group": p.provider_group,
                        "status": p.status as i32,
                    }))
                    .collect()
            };

            ok(json!({
                "elements": elements,
                "number": page,
                "size": size,
                "total": total
            })).into_response()
        }
        Err(e) => ok(json!({ "elements": [], "number": 1, "size": size, "total": 0, "_error": e.to_string() })).into_response(),
    }
}

/// Export all providers with full (unmasked) API keys
pub async fn export_providers(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let claims = match request.extensions().get::<Claims>().cloned() {
        Some(c) => c,
        None => return err(401, "Unauthorized").into_response(),
    };
    if !permission::is_org_admin(&claims) {
        return err(403, "Only org_admin can export providers").into_response();
    }
    use crate::database::dao::{ProviderEndpointDao, ProviderModelDao};
    match ProviderDao::get_all(&state.db, 0) {
        Ok(providers) => {
            let mut result: Vec<Value> = Vec::new();
            for p in &providers {
                let mut pv = serde_json::to_value(p).unwrap_or(json!({}));
                // Attach endpoints for complete export
                if let Ok(endpoints) = ProviderEndpointDao::list_by_provider(&state.db, p.id) {
                    let eps: Vec<Value> = endpoints.iter().map(|ep| json!({
                        "endpoint_type": ep.endpoint_type,
                        "api_format": ep.api_format,
                        "endpoint_path": ep.endpoint_path,
                    })).collect();
                    pv.as_object_mut().map(|obj| obj.insert("endpoints".to_string(), Value::Array(eps)));
                }
                // Attach model associations for complete export
                if let Ok(models) = ProviderModelDao::list_by_provider(&state.db, p.id) {
                    let ms: Vec<Value> = models.iter().map(|m| {
                        let mut mv = json!({
                            "model_pattern": m.model_pattern,
                            "enabled": m.enabled,
                        });
                        if let Some(ref upstream) = m.upstream_model {
                            mv.as_object_mut().map(|obj| obj.insert("upstream_model".to_string(), json!(upstream)));
                        }
                        if m.priority != 0 {
                            mv.as_object_mut().map(|obj| obj.insert("priority".to_string(), json!(m.priority)));
                        }
                        if m.weight != 0 {
                            mv.as_object_mut().map(|obj| obj.insert("weight".to_string(), json!(m.weight)));
                        }
                        mv
                    }).collect();
                    if !ms.is_empty() {
                        pv.as_object_mut().map(|obj| obj.insert("models".to_string(), Value::Array(ms)));
                    }
                }
                result.push(pv);
            }
            ok(Value::Array(result)).into_response()
        }
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

pub async fn get_provider(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match ProviderDao::get_by_id(&state.db, id) {
        Ok(Some(p)) => ok(p.with_masked_key()).into_response(),
        Ok(None) => err(404, "Provider not found").into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

pub async fn create_provider(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let (parts, body) = request.into_parts();
    let claims = match parts.extensions.get::<Claims>() {
        Some(c) => c.clone(),
        None => return err(401, "Unauthorized").into_response(),
    };
    if !permission::is_org_admin(&claims) {
        return err(403, "Only org_admin can manage providers").into_response();
    }
    let body: Value = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        Err(_) => return err(400, "Invalid request body").into_response(),
    };

    match ProviderDao::create_from_json(&state.db, &body) {
        Ok(id) => {
            let now = chrono::Utc::now().timestamp_millis();

            // Auto-create endpoints: prefer inline `endpoints` array, fallback to preset_key
            if let Some(endpoints_arr) = body.get("endpoints").and_then(|v| v.as_array()) {
                for ep in endpoints_arr {
                    let ep_type = ep.get("endpoint_type").and_then(|v| v.as_str()).unwrap_or("");
                    let api_format = ep.get("api_format").and_then(|v| v.as_str()).unwrap_or("");
                    let ep_path = ep.get("endpoint_path").and_then(|v| v.as_str()).unwrap_or("");
                    if !ep_type.is_empty() && !api_format.is_empty() && !ep_path.is_empty() {
                        let endpoint = crate::models::provider_endpoint::ProviderEndpoint {
                            created_at: now,
                            updated_at: now,
                            ..crate::models::provider_endpoint::ProviderEndpoint::new(
                                id, ep_type, api_format, ep_path,
                            )
                        };
                        let _ = crate::database::dao::ProviderEndpointDao::insert(&state.db, &endpoint);
                    }
                }
            } else if let Some(preset_key) = body.get("preset_key").and_then(|v| v.as_str()) {
                if let Ok(Some(preset)) = ProviderPresetDao::get_by_key(&state.db, preset_key) {
                    for ep in &preset.endpoints {
                        let endpoint = crate::models::provider_endpoint::ProviderEndpoint {
                            created_at: now,
                            updated_at: now,
                            ..crate::models::provider_endpoint::ProviderEndpoint::new(
                                id, &ep.endpoint_type, &ep.format, &ep.path,
                            )
                        };
                        let _ = crate::database::dao::ProviderEndpointDao::insert(&state.db, &endpoint);
                    }
                }
            }

            // Invalidate active providers list
            state.proxy_state.provider_cache.invalidate_all_active(0).await;
            match ProviderDao::get_by_id(&state.db, id) {
                Ok(Some(p)) => ok(p.with_masked_key()).into_response(),
                _ => ok(json!({ "id": id })).into_response(),
            }
        }
        Err(e) => err(400, &e.to_string()).into_response(),
    }
}

pub async fn update_provider(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let (parts, body_bytes) = request.into_parts();
    let claims = match parts.extensions.get::<Claims>() {
        Some(c) => c.clone(),
        None => return err(401, "Unauthorized").into_response(),
    };
    if !permission::is_org_admin(&claims) {
        return err(403, "Only org_admin can manage providers").into_response();
    }
    let body: Value = match axum::body::to_bytes(body_bytes, 1024 * 1024).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        Err(_) => return err(400, "Invalid request body").into_response(),
    };

    // Load old provider to get name for cache invalidation
    let old_name = ProviderDao::get_by_id(&state.db, id)
        .ok().flatten().map(|p| p.name.clone()).unwrap_or_default();
    match ProviderDao::update_from_json(&state.db, id, &body) {
        Ok(_) => {
            // Invalidate cache for old name + new state
            let new_name = body.get("name").and_then(|v| v.as_str()).unwrap_or(&old_name);
            state.proxy_state.provider_cache.invalidate_provider(id, &old_name, 0).await;
            if new_name != old_name {
                state.proxy_state.provider_cache.invalidate_provider(id, new_name, 0).await;
            }
            match ProviderDao::get_by_id(&state.db, id) {
                Ok(Some(p)) => ok(p.with_masked_key()).into_response(),
                _ => ok_void().into_response(),
            }
        }
        Err(e) => err(400, &e.to_string()).into_response(),
    }
}

pub async fn delete_provider(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    if let Some(claims) = request.extensions().get::<Claims>() {
        if !permission::is_org_admin(claims) {
            return err(403, "Only org_admin can manage providers").into_response();
        }
    }
    use crate::database::dao::ProviderEndpointDao;
    // Load provider name before deleting for cache invalidation
    let name = ProviderDao::get_by_id(&state.db, id)
        .ok().flatten().map(|p| p.name.clone()).unwrap_or_default();
    // Clean up all related tables (no foreign key cascade in DB)
    let _ = ProviderEndpointDao::delete_by_provider(&state.db, id);
    let _ = crate::database::dao::ProviderModelDao::delete_by_provider(&state.db, id);
    let _ = crate::database::dao::ProviderDao::delete_health(&state.db, id);
    match ProviderDao::delete(&state.db, id) {
        Ok(_) => {
            state.proxy_state.provider_cache.invalidate_provider(id, &name, 0).await;
            ok_void().into_response()
        }
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

pub async fn activate_provider(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    use crate::models::ProviderStatus;
    match ProviderDao::get_by_id(&state.db, id) {
        Ok(Some(mut p)) => {
            p.status = ProviderStatus::Enabled;
            match ProviderDao::update(&state.db, &p) {
                Ok(_) => {
                    state.proxy_state.provider_cache.invalidate_provider(id, &p.name, 0).await;
                    ok_void().into_response()
                }
                Err(e) => err(500, &e.to_string()).into_response(),
            }
        }
        Ok(None) => err(404, "Provider not found").into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

pub async fn deactivate_provider(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    use crate::models::ProviderStatus;
    match ProviderDao::get_by_id(&state.db, id) {
        Ok(Some(mut p)) => {
            p.status = ProviderStatus::ManuallyDisabled;
            match ProviderDao::update(&state.db, &p) {
                Ok(_) => {
                    state.proxy_state.provider_cache.invalidate_provider(id, &p.name, 0).await;
                    ok_void().into_response()
                }
                Err(e) => err(500, &e.to_string()).into_response(),
            }
        }
        Ok(None) => err(404, "Provider not found").into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

pub async fn import_providers(
    State(state): State<AdminState>,
    Json(body): Json<Vec<Value>>,
) -> impl IntoResponse {
    use crate::database::dao::{ProviderEndpointDao, ProviderModelDao, ProviderPresetDao};
    let mut created = 0;
    let mut updated = 0;
    for item in body {
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() { continue; }

        // Upsert: if provider with same name exists, update it; otherwise create
        let id = if let Ok(Some(existing)) = ProviderDao::get_by_name(&state.db, name) {
            let _ = ProviderDao::update_from_json(&state.db, existing.id, &item);
            updated += 1;
            existing.id
        } else {
            match ProviderDao::create_from_json(&state.db, &item) {
                Ok(id) => { created += 1; id }
                Err(_) => continue,
            }
        };

        // Recreate endpoints: delete old ones, insert new
        let _ = ProviderEndpointDao::delete_by_provider(&state.db, id);
        let now = chrono::Utc::now().timestamp_millis();

        if let Some(endpoints_arr) = item.get("endpoints").and_then(|v| v.as_array()) {
            for ep in endpoints_arr {
                let ep_type = ep.get("endpoint_type").and_then(|v| v.as_str()).unwrap_or("");
                let api_format = ep.get("api_format").and_then(|v| v.as_str()).unwrap_or("");
                let ep_path = ep.get("endpoint_path").and_then(|v| v.as_str()).unwrap_or("");
                if !ep_type.is_empty() && !api_format.is_empty() && !ep_path.is_empty() {
                    let endpoint = crate::models::provider_endpoint::ProviderEndpoint {
                        created_at: now,
                        updated_at: now,
                        ..crate::models::provider_endpoint::ProviderEndpoint::new(
                            id, ep_type, api_format, ep_path,
                        )
                    };
                    let _ = ProviderEndpointDao::insert(&state.db, &endpoint);
                }
            }
        } else if let Some(preset_key) = item.get("preset_key").and_then(|v| v.as_str()) {
            if let Ok(Some(preset)) = ProviderPresetDao::get_by_key(&state.db, preset_key) {
                for ep in &preset.endpoints {
                    let endpoint = crate::models::provider_endpoint::ProviderEndpoint {
                        created_at: now,
                        updated_at: now,
                        ..crate::models::provider_endpoint::ProviderEndpoint::new(
                            id, &ep.endpoint_type, &ep.format, &ep.path,
                        )
                    };
                    let _ = ProviderEndpointDao::insert(&state.db, &endpoint);
                }
            }
        } else {
            // No endpoints in JSON and no preset_key — try to match preset by provider name
            if let Ok(Some(preset)) = ProviderPresetDao::get_by_key(&state.db, name) {
                for ep in &preset.endpoints {
                    let endpoint = crate::models::provider_endpoint::ProviderEndpoint {
                        created_at: now,
                        updated_at: now,
                        ..crate::models::provider_endpoint::ProviderEndpoint::new(
                            id, &ep.endpoint_type, &ep.format, &ep.path,
                        )
                    };
                    let _ = ProviderEndpointDao::insert(&state.db, &endpoint);
                }
            }
        }

        // Recreate model associations: delete old ones, insert new
        if let Some(models_arr) = item.get("models").and_then(|v| v.as_array()) {
            let _ = ProviderModelDao::delete_by_provider(&state.db, id);
            // Get org_id from provider or default to 1
            let org_id = item.get("org_id").and_then(|v| v.as_i64()).unwrap_or(1);
            for m in models_arr {
                let model_pattern = m.get("model_pattern").and_then(|v| v.as_str()).unwrap_or("");
                if model_pattern.is_empty() { continue; }
                let upstream_model = m.get("upstream_model").and_then(|v| v.as_str()).map(|s| s.to_string());
                let priority = m.get("priority").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let weight = m.get("weight").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let enabled = m.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                let pm = crate::models::provider_model::ProviderModel {
                    id: 0,
                    org_id,
                    provider_id: id,
                    model_pattern: model_pattern.to_string(),
                    upstream_model,
                    priority,
                    weight,
                    enabled,
                    created_at: now,
                    updated_at: now,
                };
                let _ = ProviderModelDao::insert(&state.db, &pm);
            }
        }
    }
    // Invalidate provider cache
    state.proxy_state.provider_cache.invalidate_all_active(0).await;
    ok(json!({ "created": created, "updated": updated, "total": created + updated }))
}

// ============================================================
//  Tokens (API Keys)
// ============================================================

pub async fn list_tokens(
    State(state): State<AdminState>,
) -> impl IntoResponse {
    match ApiKeyDao::list_all(&state.db) {
        Ok(keys) => {
            let tokens: Vec<Value> = keys.iter().map(|k| json!({
                "id": k.id,
                "name": k.name,
                "token": k.key_prefix,
                "is_active": k.is_active,
                "expires_at": k.expires_at,
                "created_at": k.created_at,
                "last_used_at": k.last_used_at,
            })).collect();
            ok(Value::Array(tokens))
        }
        Err(_) => ok(json!([])),
    }
}

pub async fn get_token(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match ApiKeyDao::get_by_id(&state.db, id) {
        Ok(Some(k)) => ok(json!({
            "id": k.id,
            "name": k.name,
            "token": k.key_prefix,
            "is_active": k.is_active,
            "expires_at": k.expires_at,
            "created_at": k.created_at,
            "last_used_at": k.last_used_at,
        })).into_response(),
        Ok(None) => err(404, "Token not found").into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    pub expires_at: Option<String>,
}

pub async fn create_token(
    State(state): State<AdminState>,
    Json(body): Json<CreateTokenRequest>,
) -> impl IntoResponse {
    use crate::models::ApiKey;

    let (raw_token, key) = ApiKey::generate(0, &body.name, "default");
    let mut key = key;
    key.expires_at = body.expires_at.and_then(|s| s.parse::<i64>().ok());

    match ApiKeyDao::insert(&state.db, &key) {
        Ok(id) => ok(json!({
            "id": id,
            "name": key.name,
            "token": raw_token,
            "is_active": true,
            "expires_at": key.expires_at,
            "created_at": key.created_at,
            "last_used_at": null,
        })).into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

pub async fn update_token(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Some(active) = body.get("is_active").and_then(|v| v.as_bool()) {
        let _ = ApiKeyDao::set_active(&state.db, id, active);
    }
    match ApiKeyDao::get_by_id(&state.db, id) {
        Ok(Some(k)) => ok(json!({
            "id": k.id,
            "name": k.name,
            "token": k.key_prefix,
            "is_active": k.is_active,
            "expires_at": k.expires_at,
            "created_at": k.created_at,
            "last_used_at": k.last_used_at,
        })).into_response(),
        _ => ok_void().into_response(),
    }
}

pub async fn delete_token(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match ApiKeyDao::delete(&state.db, id) {
        Ok(_) => ok_void().into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

pub async fn regenerate_token(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    use crate::models::ApiKey;

    // Get existing token to preserve name
    match ApiKeyDao::get_by_id(&state.db, id) {
        Ok(Some(existing)) => {
            // Delete old and create new
            let _ = ApiKeyDao::delete(&state.db, id);
            let (raw_token, key) = ApiKey::generate(existing.user_id, &existing.name, &existing.provider_group);
            let mut key = key;
            key.expires_at = existing.expires_at;

            match ApiKeyDao::insert(&state.db, &key) {
                Ok(new_id) => ok(json!({
                    "id": new_id,
                    "name": key.name,
                    "token": raw_token,
                    "is_active": true,
                    "expires_at": key.expires_at,
                    "created_at": key.created_at,
                    "last_used_at": null,
                })).into_response(),
                Err(e) => err(500, &e.to_string()).into_response(),
            }
        }
        _ => err(404, "Token not found").into_response(),
    }
}

// ============================================================
//  Stats
// ============================================================

#[derive(Deserialize)]
pub struct StatsQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

pub async fn token_stats_one(
    State(_state): State<AdminState>,
    Path(_id): Path<i64>,
    Query(_q): Query<StatsQuery>,
) -> impl IntoResponse {
    // Placeholder — will be backed by request_logs aggregation in Phase 3
    ok(json!({
        "token_id": _id,
        "token_name": "",
        "total_requests": 0,
        "total_input_tokens": 0,
        "total_output_tokens": 0,
        "total_cost": 0.0,
        "by_provider": []
    }))
}

pub async fn token_stats_all(
    State(_state): State<AdminState>,
    Query(_q): Query<StatsQuery>,
) -> impl IntoResponse {
    ok(json!([]))
}

pub async fn stats_summary(
    State(state): State<AdminState>,
    Query(_q): Query<StatsQuery>,
) -> impl IntoResponse {
    use crate::database::dao::StatsDao;

    // Get active providers count
    let active_providers = ProviderDao::get_all(&state.db, 0)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.status == crate::models::ProviderStatus::Enabled)
        .count();

    // Get active API keys count
    let active_keys = ApiKeyDao::list_all(&state.db)
        .unwrap_or_default()
        .into_iter()
        .filter(|k| k.is_active)
        .count();

    // Today's time range (UTC)
    let now = chrono::Utc::now();
    let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
    let from_ts = today_start.and_utc().timestamp_millis();
    let to_ts = now.timestamp_millis();

    // Get today's summary from request_logs
    let summary = StatsDao::get_summary(&state.db, from_ts, to_ts).unwrap_or_else(|_| {
        crate::models::UsageSummary {
            total_requests: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cache_creation_tokens: 0,
            total_cache_read_tokens: 0,
            total_tokens: 0,
            total_cost: 0.0,
            avg_cost_per_request: 0.0,
            by_provider: vec![],
        }
    });

    // Get hourly breakdown (last 24 hours)
    let requests_per_hour = StatsDao::get_hourly_requests(&state.db, from_ts, to_ts)
        .unwrap_or_default();

    // Get top models
    let top_models = StatsDao::get_top_models(&state.db, from_ts, to_ts, 10)
        .unwrap_or_default();

    // Requests by provider type (use by_provider data)
    let requests_by_provider_type: Vec<Value> = summary.by_provider.iter()
        .map(|p| json!({ "provider_type": p.provider_name, "count": p.requests }))
        .collect();

    ok(json!({
        "total_requests_today": summary.total_requests,
        "active_providers": active_providers,
        "active_api_keys": active_keys,
        "total_tokens_today": summary.total_tokens,
        "total_input_tokens_today": summary.total_input_tokens,
        "total_output_tokens_today": summary.total_output_tokens,
        "total_cache_creation_tokens_today": summary.total_cache_creation_tokens,
        "total_cache_read_tokens_today": summary.total_cache_read_tokens,
        "total_cost_today": summary.total_cost,
        "avg_cost_per_request": summary.avg_cost_per_request,
        "requests_per_hour": requests_per_hour,
        "requests_by_provider_type": requests_by_provider_type,
        "top_models": top_models,
        "by_provider": summary.by_provider,
    }))
}

// ============================================================
//  Logs
// ============================================================

#[derive(Deserialize)]
pub struct LogsQuery {
    pub page: Option<u32>,
    pub size: Option<u32>,
    pub provider_id: Option<i64>,
    pub token_name: Option<String>,
    pub model: Option<String>,
    pub status: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

pub async fn list_logs(
    State(state): State<AdminState>,
    Query(q): Query<LogsQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    use crate::database::dao::StatsDao;

    let claims = match request.extensions().get::<Claims>().cloned() {
        Some(c) => c,
        None => return err(401, "Unauthorized").into_response(),
    };

    let page = q.page.unwrap_or(1).max(1);
    let size = q.size.unwrap_or(20).min(100);
    let offset = ((page - 1) * size) as i64;
    let limit = size as i64;

    // Apply user_id scope based on role
    let scope_user_id = match permission::resource_scope(&claims) {
        ResourceScope::Global => None,
        ResourceScope::All => None,
        ResourceScope::User(uid) => Some(uid),
    };

    let filters = crate::database::dao::LogFilters {
        provider_id: q.provider_id,
        token_name: q.token_name.clone(),
        model: q.model.clone(),
        status: q.status.clone(),
        user_id: scope_user_id,
    };

    let total = match StatsDao::count_request_logs_filtered(&state.db, &filters) {
        Ok(c) => c,
        Err(_) => 0,
    };

    let logs = match StatsDao::get_request_logs_filtered(
        &state.db,
        &filters,
        limit,
        offset,
    ) {
        Ok(logs) => logs,
        Err(_) => vec![],
    };

    let elements: Vec<Value> = logs.iter().map(|l| json!({
        "id": l.id,
        "provider_id": l.provider_id,
        "provider_name": l.provider_name,
        "model": l.model,
        "status": l.status_code,
        "duration_ms": l.duration_ms,
        "first_token_ms": l.first_token_ms,
        "input_tokens": l.input_tokens,
        "output_tokens": l.output_tokens,
        "total_tokens": l.total_tokens,
        "estimated_cost": l.estimated_cost,
        "error_message": l.error_message,
        "request_path": l.request_path,
        "request_method": l.request_method,
        "token_name": l.token_name,
        "api_key_name": l.api_key_name,
        "user_id": l.user_id,
        "client_ip": l.client_ip,
        "request_id": l.request_id,
        "request_time": l.request_time,
        "created_at": l.request_time,
    })).collect();

    ok(json!({
        "elements": elements,
        "number": page,
        "size": size,
        "total": total
    })).into_response()
}

// ============================================================
//  Config (System Settings)
// ============================================================

pub async fn get_config(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let claims = match request.extensions().get::<Claims>().cloned() {
        Some(c) => c,
        None => return err(401, "Unauthorized").into_response(),
    };
    if !permission::is_org_admin(&claims) {
        return err(403, "Only org_admin can view config").into_response();
    }
    match SystemSettingsDao::list_all(&state.db) {
        Ok(settings) => {
            let mut map = serde_json::Map::new();
            for s in settings {
                map.insert(s.key, s.value.clone());
            }
            ok(Value::Object(map)).into_response()
        }
        Err(_) => ok(json!({})).into_response(),
    }
}

pub async fn update_config(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let (parts, body) = request.into_parts();
    let claims = match parts.extensions.get::<Claims>() {
        Some(c) => c.clone(),
        None => return err(401, "Unauthorized").into_response(),
    };
    if !permission::is_org_admin(&claims) {
        return err(403, "Only org_admin can update config").into_response();
    }
    let body: Value = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        Err(_) => return err(400, "Invalid request body").into_response(),
    };
    if let Value::Object(map) = body {
        for (key, value) in map {
            let setting = crate::models::SystemSetting {
                key,
                value,
                description: None,
                updated_at: chrono::Utc::now().timestamp_millis(),
            };
            let _ = SystemSettingsDao::set(&state.db, &setting);
        }
    }
    ok_void().into_response()
}

// ============================================================
//  Proxy Status
// ============================================================

pub async fn proxy_status(
    State(state): State<AdminState>,
) -> impl IntoResponse {
    let proxy = &state.proxy_state;
    let local_count = *proxy.request_count.read().await;
    let provider = proxy.current_provider.read().await;
    let queue = proxy.failover_queue.read().await;

    // Prefer global count from Redis (distributed mode); fall back to local
    let count = proxy.distributed.get_request_count().await.unwrap_or(local_count);

    ok(json!({
        "status": "running",
        "request_count": count,
        "local_request_count": local_count,
        "current_provider": provider.as_ref().map(|p| &p.name),
        "failover_queue_size": queue.len(),
    }))
}

pub async fn reset_circuit_breaker(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let breakers = state.proxy_state.circuit_breakers.read().await;
    if let Some(cb) = breakers.get(&id) {
        cb.reset().await;
        ok_void().into_response()
    } else {
        err(404, "Circuit breaker not found").into_response()
    }
}

// ============================================================
//  Quota (placeholder — full implementation in Phase 3)
// ============================================================

pub async fn list_quota_rules(State(state): State<AdminState>) -> Response {
    #[cfg(feature = "sqlite")]
    {
        let conn = state.db.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, name, quota_group, dimension, model_pattern, limit_type, limit_value,
                    window_secs, enabled, priority, window_tz, window_type, provider_name_pattern, skip_owner
             FROM quota_rules ORDER BY priority ASC"
        ) {
            Ok(s) => s,
            Err(e) => return err(500, &format!("DB error: {}", e)).into_response(),
        };

        let rules: Vec<serde_json::Value> = match stmt.query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "name": row.get::<_, String>(1)?,
                "quota_group": row.get::<_, Option<String>>(2)?,
                "dimension": row.get::<_, String>(3)?,
                "model_pattern": row.get::<_, String>(4)?,
                "limit_type": row.get::<_, String>(5)?,
                "limit_value": row.get::<_, i64>(6)?,
                "window_secs": row.get::<_, i64>(7)?,
                "enabled": row.get::<_, i32>(8)? != 0,
                "priority": row.get::<_, i64>(9)?,
                "window_tz": row.get::<_, Option<String>>(10)?,
                "window_type": row.get::<_, Option<String>>(11)?.unwrap_or_else(|| "fixed".to_string()),
                "provider_name_pattern": row.get::<_, Option<String>>(12)?,
                "skip_owner": row.get::<_, i32>(13).unwrap_or(0) != 0,
            }))
        }) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(_) => vec![],
        };

        return ok(json!(rules)).into_response();
    }

    #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
    {
        use sqlx::Row;
        let rows = match sqlx::query(
            "SELECT id, name, quota_group, dimension, model_pattern, limit_type, limit_value,
                    window_secs, enabled, priority, window_tz, window_type, provider_name_pattern, skip_owner
             FROM quota_rules ORDER BY priority ASC"
        )
        .fetch_all(&state.db.pool)
        .await {
            Ok(r) => r,
            Err(e) => return err(500, &format!("DB error: {}", e)).into_response(),
        };

        let rules: Vec<serde_json::Value> = rows.iter().map(|row| {
            json!({
                "id": row.get::<i64, _>("id"),
                "name": row.get::<String, _>("name"),
                "quota_group": row.get::<Option<String>, _>("quota_group"),
                "dimension": row.get::<String, _>("dimension"),
                "model_pattern": row.get::<String, _>("model_pattern"),
                "limit_type": row.get::<String, _>("limit_type"),
                "limit_value": row.get::<i64, _>("limit_value"),
                "window_secs": row.get::<i64, _>("window_secs"),
                "enabled": row.get::<i32, _>("enabled") != 0,
                "priority": row.get::<i64, _>("priority"),
                "window_tz": row.get::<Option<String>, _>("window_tz"),
                "window_type": row.get::<Option<String>, _>("window_type").unwrap_or_else(|| "fixed".to_string()),
                "provider_name_pattern": row.get::<Option<String>, _>("provider_name_pattern"),
                "skip_owner": row.get::<i32, _>("skip_owner") != 0,
            })
        }).collect();

        return ok(json!(rules)).into_response();
    }

    #[allow(unreachable_code)]
    err(500, "No database backend configured").into_response()
}

pub async fn create_quota_rule(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let claims = match parts.extensions.get::<Claims>() {
        Some(c) => c.clone(),
        None => return err(401, "Unauthorized").into_response(),
    };
    if !permission::is_org_admin(&claims) {
        return err(403, "Only org_admin can manage quota rules").into_response();
    }
    let body: serde_json::Value = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        Err(_) => return err(400, "Invalid request body").into_response(),
    };
    let name = body["name"].as_str().unwrap_or("").to_string();
    let quota_group = body["quota_group"].as_str().map(|s| s.to_string());
    let dimension = body["dimension"].as_str().unwrap_or("key").to_string();
    let model_pattern = body["model_pattern"].as_str().unwrap_or("*").to_string();
    let limit_type = body["limit_type"].as_str().unwrap_or("tokens").to_string();
    let limit_value = body["limit_value"].as_i64().unwrap_or(0);
    let window_secs = body["window_secs"].as_i64().unwrap_or(86400);
    let enabled = body["enabled"].as_bool().unwrap_or(true);
    let priority = body["priority"].as_i64().unwrap_or(100);
    let window_tz = body["window_tz"].as_str().map(|s| s.to_string());
    let window_type = body["window_type"].as_str().unwrap_or("fixed").to_string();
    let provider_name_pattern = body["provider_name_pattern"].as_str().map(|s| s.to_string());
    let skip_owner = body["skip_owner"].as_bool().unwrap_or(false);

    if name.is_empty() {
        return err(400, "name is required").into_response();
    }

    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    #[cfg(feature = "sqlite")]
    {
        match state.db.execute(
            "INSERT INTO quota_rules (name, quota_group, dimension, model_pattern, limit_type, limit_value,
                    window_secs, enabled, priority, window_tz, window_type, provider_name_pattern, skip_owner, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            rusqlite::params![
                name, quota_group, dimension, model_pattern, limit_type, limit_value,
                window_secs, enabled as i32, priority, window_tz, window_type, provider_name_pattern,
                skip_owner as i32, now, now
            ],
        ) {
            Ok(_) => {
                state.proxy_state.quota_checker.invalidate_rules_cache();
                let conn = state.db.conn.lock().unwrap();
                let id = conn.last_insert_rowid();
                return ok(json!({ "id": id })).into_response();
            }
            Err(e) => return err(500, &format!("Failed to create quota rule: {}", e)).into_response(),
        }
    }

    #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
    {
        match sqlx::query(
            "INSERT INTO quota_rules (name, quota_group, dimension, model_pattern, limit_type, limit_value,
                    window_secs, enabled, priority, window_tz, window_type, provider_name_pattern, skip_owner, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&name)
        .bind(&quota_group)
        .bind(&dimension)
        .bind(&model_pattern)
        .bind(&limit_type)
        .bind(limit_value)
        .bind(window_secs)
        .bind(enabled as i32)
        .bind(priority)
        .bind(&window_tz)
        .bind(&window_type)
        .bind(&provider_name_pattern)
        .bind(skip_owner as i32)
        .bind(&now)
        .bind(&now)
        .execute(&state.db.pool)
        .await {
            Ok(result) => {
                state.proxy_state.quota_checker.invalidate_rules_cache();
                let id = result.last_insert_id() as i64;
                return ok(json!({ "id": id })).into_response();
            }
            Err(e) => return err(500, &format!("Failed to create quota rule: {}", e)).into_response(),
        }
    }

    #[allow(unreachable_code)]
    err(500, "No database backend configured").into_response()
}

pub async fn update_quota_rule(
    State(state): State<AdminState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    request: axum::extract::Request,
) -> Response {
    let claims = match request.extensions().get::<Claims>().cloned() {
        Some(c) => c,
        None => return err(401, "Unauthorized").into_response(),
    };
    if !permission::is_org_admin(&claims) {
        return err(403, "Only org_admin can manage quota rules").into_response();
    }
    let body: serde_json::Value = match axum::body::to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        Err(_) => return err(400, "Invalid request body").into_response(),
    };
    let name = body["name"].as_str().unwrap_or("").to_string();
    let quota_group = body["quota_group"].as_str().map(|s| s.to_string());
    let dimension = body["dimension"].as_str().unwrap_or("key").to_string();
    let model_pattern = body["model_pattern"].as_str().unwrap_or("*").to_string();
    let limit_type = body["limit_type"].as_str().unwrap_or("tokens").to_string();
    let limit_value = body["limit_value"].as_i64().unwrap_or(0);
    let window_secs = body["window_secs"].as_i64().unwrap_or(86400);
    let enabled = body["enabled"].as_bool().unwrap_or(true);
    let priority = body["priority"].as_i64().unwrap_or(100);
    let window_tz = body["window_tz"].as_str().map(|s| s.to_string());
    let window_type = body["window_type"].as_str().unwrap_or("fixed").to_string();
    let provider_name_pattern = body["provider_name_pattern"].as_str().map(|s| s.to_string());
    let skip_owner = body["skip_owner"].as_bool().unwrap_or(false);

    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    #[cfg(feature = "sqlite")]
    {
        match state.db.execute(
            "UPDATE quota_rules SET name=?1, quota_group=?2, dimension=?3, model_pattern=?4,
                    limit_type=?5, limit_value=?6, window_secs=?7, enabled=?8, priority=?9,
                    window_tz=?10, window_type=?11, provider_name_pattern=?12, skip_owner=?13, updated_at=?14
             WHERE id=?15",
            rusqlite::params![
                name, quota_group, dimension, model_pattern, limit_type, limit_value,
                window_secs, enabled as i32, priority, window_tz, window_type, provider_name_pattern,
                skip_owner as i32, now, id
            ],
        ) {
            Ok(_) => {
                state.proxy_state.quota_checker.invalidate_rules_cache();
                return ok_void().into_response();
            }
            Err(e) => return err(500, &format!("Failed to update quota rule: {}", e)).into_response(),
        }
    }

    #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
    {
        match sqlx::query(
            "UPDATE quota_rules SET name=?, quota_group=?, dimension=?, model_pattern=?,
                    limit_type=?, limit_value=?, window_secs=?, enabled=?, priority=?,
                    window_tz=?, window_type=?, provider_name_pattern=?, skip_owner=?, updated_at=?
             WHERE id=?"
        )
        .bind(&name)
        .bind(&quota_group)
        .bind(&dimension)
        .bind(&model_pattern)
        .bind(&limit_type)
        .bind(limit_value)
        .bind(window_secs)
        .bind(enabled as i32)
        .bind(priority)
        .bind(&window_tz)
        .bind(&window_type)
        .bind(&provider_name_pattern)
        .bind(skip_owner as i32)
        .bind(&now)
        .bind(id)
        .execute(&state.db.pool)
        .await {
            Ok(_) => {
                state.proxy_state.quota_checker.invalidate_rules_cache();
                return ok_void().into_response();
            }
            Err(e) => return err(500, &format!("Failed to update quota rule: {}", e)).into_response(),
        }
    }

    #[allow(unreachable_code)]
    err(500, "No database backend configured").into_response()
}

pub async fn delete_quota_rule(
    State(state): State<AdminState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    request: axum::extract::Request,
) -> Response {
    let claims = match request.extensions().get::<Claims>().cloned() {
        Some(c) => c,
        None => return err(401, "Unauthorized").into_response(),
    };
    if !permission::is_org_admin(&claims) {
        return err(403, "Only org_admin can manage quota rules").into_response();
    }
    #[cfg(feature = "sqlite")]
    {
        match state.db.execute(
            "DELETE FROM quota_rules WHERE id=?1",
            rusqlite::params![id],
        ) {
            Ok(_) => {
                state.proxy_state.quota_checker.invalidate_rules_cache();
                return ok_void().into_response();
            }
            Err(e) => return err(500, &format!("Failed to delete quota rule: {}", e)).into_response(),
        }
    }

    #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
    {
        match sqlx::query("DELETE FROM quota_rules WHERE id = ?")
            .bind(id)
            .execute(&state.db.pool)
            .await {
            Ok(_) => {
                state.proxy_state.quota_checker.invalidate_rules_cache();
                return ok_void().into_response();
            }
            Err(e) => return err(500, &format!("Failed to delete quota rule: {}", e)).into_response(),
        }
    }

    #[allow(unreachable_code)]
    err(500, "No database backend configured").into_response()
}

pub async fn get_quota_usage(
    State(state): State<AdminState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let subject = params.get("subject").cloned().unwrap_or_default();
    let model_filter = params.get("model").map(|s| s.as_str());

    if subject.is_empty() {
        return ok(json!([])).into_response();
    }

    match state.proxy_state.quota_checker.query_usage(&subject, model_filter).await {
        Ok(entries) => {
            let data: Vec<Value> = entries.iter().map(|e| json!({
                "rule_id": e.rule_id,
                "rule_name": e.rule_name,
                "subject_key": e.subject_key,
                "model": e.model,
                "used": e.used,
                "limit": e.limit,
                "remaining": e.remaining,
                "limit_type": e.limit_type,
                "window_secs": e.window_secs,
            })).collect();
            ok(json!(data)).into_response()
        }
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

pub async fn get_quota_groups(State(state): State<AdminState>) -> Response {
    #[cfg(feature = "sqlite")]
    {
        let conn = state.db.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT DISTINCT quota_group FROM quota_rules WHERE quota_group IS NOT NULL ORDER BY quota_group"
        ) {
            Ok(s) => s,
            Err(_) => return ok(json!([])).into_response(),
        };

        let groups: Vec<String> = match stmt.query_map([], |row| row.get(0)) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(_) => vec![],
        };

        return ok(json!(groups)).into_response();
    }

    #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
    {
        use sqlx::Row;
        let rows = match sqlx::query(
            "SELECT DISTINCT quota_group FROM quota_rules WHERE quota_group IS NOT NULL ORDER BY quota_group"
        )
        .fetch_all(&state.db.pool)
        .await {
            Ok(r) => r,
            Err(_) => return ok(json!([])).into_response(),
        };

        let groups: Vec<String> = rows.iter().map(|r| r.get("quota_group")).collect();
        return ok(json!(groups)).into_response();
    }

    #[allow(unreachable_code)]
    ok(json!([])).into_response()
}

pub async fn invalidate_quota_cache(State(state): State<AdminState>) -> impl IntoResponse {
    state.proxy_state.quota_checker.invalidate_rules_cache();
    ok_void()
}

// ============================================================
//  Health Check
// ============================================================

pub async fn health_check() -> impl IntoResponse {
    Json(json!({
        "code": 0,
        "message": "ok",
        "timestamp": chrono::Utc::now().timestamp_millis()
    }))
}

// ============================================================
//  Auth (Login / Me)
// ============================================================

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub org_id: Option<i64>,
}

pub async fn login(
    State(state): State<AdminState>,
    Json(body): Json<LoginRequest>,
) -> impl IntoResponse {
    // Global username lookup — supports super_admin (org_id=0) and unique usernames
    let user = match UserDao::get_by_username_any(&state.db, &body.username) {
        Ok(users) => {
            match users.len() {
                0 => return err(401, "Invalid username or password").into_response(),
                1 => users.into_iter().next().unwrap(),
                _ => {
                    // Multiple users with same username across orgs — need org_id
                    match body.org_id {
                        Some(oid) => {
                            match users.into_iter().find(|u| u.org_id == oid) {
                                Some(u) => u,
                                None => return err(401, "Invalid username or password").into_response(),
                            }
                        }
                        None => return err(400, "Multiple accounts found. Please specify org_id.").into_response(),
                    }
                }
            }
        }
        Err(_) => return err(500, "Database error").into_response(),
    };

    // Verify password
    match auth::verify_password(&body.password, &user.password_hash) {
        Ok(true) => {}
        _ => return err(401, "Invalid username or password").into_response(),
    }

    if !user.is_active {
        return err(403, "Account is disabled").into_response();
    }

    // Update last login time
    let _ = UserDao::update_last_login(&state.db, user.id);

    // Create JWT token
    let expiry_hours = state.config.admin.jwt_expiry_hours;
    let token = match auth::create_token(
        user.id, user.org_id, &user.role.to_string(), &state.jwt_secret, expiry_hours,
    ) {
        Ok(t) => t,
        Err(_) => return err(500, "Failed to create token").into_response(),
    };

    ok(json!({
        "token": token,
        "expires_in": expiry_hours * 3600,
        "user": {
            "id": user.id,
            "username": user.username,
            "display_name": user.display_name,
            "email": user.email,
            "role": user.role.to_string(),
            "org_id": user.org_id,
        }
    })).into_response()
}

pub async fn auth_me(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let claims = match request.extensions().get::<Claims>() {
        Some(c) => c.clone(),
        None => return err(401, "Not authenticated").into_response(),
    };

    match UserDao::get_by_id(&state.db, claims.user_id) {
        Ok(Some(user)) => ok(json!({
            "id": user.id,
            "username": user.username,
            "display_name": user.display_name,
            "email": user.email,
            "role": user.role.to_string(),
            "org_id": user.org_id,
            "provider_group": user.provider_group,
            "is_active": user.is_active,
            "last_login_at": user.last_login_at,
        })).into_response(),
        _ => err(404, "User not found").into_response(),
    }
}

// ============================================================
//  Organizations
// ============================================================

pub async fn list_organizations(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let claims = match request.extensions().get::<Claims>().cloned() {
        Some(c) => c,
        None => return err(401, "Unauthorized").into_response(),
    };
    if !permission::is_org_admin(&claims) {
        return err(403, "Only org_admin can manage organizations").into_response();
    }
    match OrganizationDao::list_all(&state.db) {
        Ok(orgs) => ok(orgs).into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub display_name: Option<String>,
}

pub async fn create_organization(
    State(state): State<AdminState>,
    Json(body): Json<CreateOrganizationRequest>,
) -> impl IntoResponse {
    let org = Organization {
        id: 0,
        name: body.name,
        display_name: body.display_name,
        is_active: true,
        settings: None,
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
    };

    match OrganizationDao::insert(&state.db, &org) {
        Ok(id) => match OrganizationDao::get_by_id(&state.db, id) {
            Ok(Some(o)) => ok(o).into_response(),
            _ => ok(json!({ "id": id })).into_response(),
        },
        Err(e) => err(400, &e.to_string()).into_response(),
    }
}

pub async fn update_organization(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let mut org = match OrganizationDao::get_by_id(&state.db, id) {
        Ok(Some(o)) => o,
        Ok(None) => return err(404, "Organization not found").into_response(),
        Err(e) => return err(500, &e.to_string()).into_response(),
    };

    if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
        org.name = name.to_string();
    }
    if let Some(display_name) = body.get("display_name").and_then(|v| v.as_str()) {
        org.display_name = Some(display_name.to_string());
    }
    if let Some(active) = body.get("is_active").and_then(|v| v.as_bool()) {
        org.is_active = active;
    }
    if let Some(settings) = body.get("settings") {
        org.settings = Some(settings.clone());
    }

    match OrganizationDao::update(&state.db, &org) {
        Ok(_) => ok(org).into_response(),
        Err(e) => err(400, &e.to_string()).into_response(),
    }
}

// ============================================================
//  Users
// ============================================================

pub async fn list_users(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let claims = match request.extensions().get::<Claims>().cloned() {
        Some(c) => c,
        None => return err(401, "Unauthorized").into_response(),
    };

    let users_result = match permission::resource_scope(&claims) {
        ResourceScope::Global => UserDao::list_all(&state.db),
        ResourceScope::All => UserDao::list_all(&state.db),
        ResourceScope::User(user_id) => {
            match UserDao::get_by_id(&state.db, user_id) {
                Ok(Some(u)) => Ok(vec![u]),
                Ok(None) => Ok(vec![]),
                Err(e) => Err(e),
            }
        }
    };

    match users_result {
        Ok(users) => {
            // Strip password_hash from response
            let safe_users: Vec<Value> = users.iter().map(|u| json!({
                "id": u.id,
                "org_id": u.org_id,
                "username": u.username,
                "display_name": u.display_name,
                "email": u.email,
                "role": u.role.to_string(),
                "provider_group": u.provider_group,
                "is_active": u.is_active,
                "last_login_at": u.last_login_at,
                "created_at": u.created_at,
                "updated_at": u.updated_at,
            })).collect();
            ok(safe_users).into_response()
        }
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub org_id: Option<i64>,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub role: Option<String>,
    pub provider_group: Option<String>,
}

pub async fn create_user(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let claims = request.extensions().get::<Claims>().cloned();
    let (body_bytes, _) = match axum::body::to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => (b, ()),
        Err(_) => return err(400, "Invalid request body").into_response(),
    };
    let body: CreateUserRequest = match serde_json::from_slice(&body_bytes) {
        Ok(b) => b,
        Err(e) => return err(400, &e.to_string()).into_response(),
    };

    // Hash password
    let password_hash = match auth::hash_password(&body.password) {
        Ok(h) => h,
        Err(_) => return err(500, "Failed to hash password").into_response(),
    };

    let org_id = body.org_id.unwrap_or_else(|| claims.map(|c| c.org_id).unwrap_or(1));
    let role = body.role
        .and_then(|r| r.parse::<UserRole>().ok())
        .unwrap_or(UserRole::User);

    let mut user = User::new(org_id, body.username, password_hash);
    user.display_name = body.display_name;
    user.email = body.email;
    user.role = role;
    user.provider_group = body.provider_group.unwrap_or("default".to_string());

    match UserDao::insert(&state.db, &user) {
        Ok(id) => ok(json!({
            "id": id,
            "username": user.username,
            "org_id": user.org_id,
            "role": user.role.to_string(),
            "is_active": true,
        })).into_response(),
        Err(e) => err(400, &e.to_string()).into_response(),
    }
}

pub async fn update_user(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let mut user = match UserDao::get_by_id(&state.db, id) {
        Ok(Some(u)) => u,
        Ok(None) => return err(404, "User not found").into_response(),
        Err(e) => return err(500, &e.to_string()).into_response(),
    };

    if let Some(display_name) = body.get("display_name").and_then(|v| v.as_str()) {
        user.display_name = Some(display_name.to_string());
    }
    if let Some(email) = body.get("email").and_then(|v| v.as_str()) {
        user.email = Some(email.to_string());
    }
    if let Some(role) = body.get("role").and_then(|v| v.as_str()) {
        if let Ok(r) = role.parse::<UserRole>() {
            user.role = r;
        }
    }
    if let Some(active) = body.get("is_active").and_then(|v| v.as_bool()) {
        user.is_active = active;
    }
    if let Some(group) = body.get("provider_group").and_then(|v| v.as_str()) {
        user.provider_group = group.to_string();
    }
    // Password update
    if let Some(password) = body.get("password").and_then(|v| v.as_str()) {
        if !password.is_empty() {
            match auth::hash_password(password) {
                Ok(h) => user.password_hash = h,
                Err(_) => return err(500, "Failed to hash password").into_response(),
            }
        }
    }

    match UserDao::update(&state.db, &user) {
        Ok(_) => ok(json!({
            "id": user.id,
            "username": user.username,
            "org_id": user.org_id,
            "role": user.role.to_string(),
            "is_active": user.is_active,
        })).into_response(),
        Err(e) => err(400, &e.to_string()).into_response(),
    }
}

pub async fn delete_user(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match UserDao::delete(&state.db, id) {
        Ok(_) => ok_void().into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

// ============================================================
//  API Keys
// ============================================================

pub async fn list_keys(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let claims = match request.extensions().get::<Claims>().cloned() {
        Some(c) => c,
        None => return err(401, "Unauthorized").into_response(),
    };

    let keys = match permission::resource_scope(&claims) {
        ResourceScope::Global => ApiKeyDao::list_all(&state.db),
        ResourceScope::All => ApiKeyDao::list_all(&state.db),
        ResourceScope::User(user_id) => ApiKeyDao::list_by_user(&state.db, user_id),
    };

    match keys {
        Ok(keys) => {
            let safe_keys: Vec<Value> = keys.iter().map(|k| {
                // Decrypt the full key if available
                let full_key = k.encrypted_key.as_ref()
                    .and_then(|enc| crate::crypto::decrypt_if_encrypted(enc).ok());
                json!({
                    "id": k.id,
                    "user_id": k.user_id,
                    "name": k.name,
                    "key_prefix": k.key_prefix,
                    "key_suffix": k.key_suffix,
                    "full_key": full_key,
                    "provider_group": k.provider_group,
                    "quota_limit": k.quota_limit,
                    "quota_group": k.quota_group,
                    "used_tokens": k.used_tokens,
                    "rpm_limit": k.rpm_limit,
                    "is_active": k.is_active,
                    "expires_at": k.expires_at,
                    "last_used_at": k.last_used_at,
                    "created_at": k.created_at,
                    "updated_at": k.updated_at,
                })
            }).collect();
            ok(safe_keys).into_response()
        }
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct CreateKeyRequest {
    pub name: String,
    pub provider_group: Option<String>,
    pub quota_limit: Option<i64>,
    pub quota_group: Option<String>,
    pub rpm_limit: Option<i32>,
    pub tpm_limit: Option<i32>,
    pub expires_at: Option<String>,
    pub user_id: Option<i64>,
}

pub async fn create_key(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let claims = request.extensions().get::<Claims>().cloned();
    let (body_bytes, _) = match axum::body::to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => (b, ()),
        Err(_) => return err(400, "Invalid request body").into_response(),
    };
    let body: CreateKeyRequest = match serde_json::from_slice(&body_bytes) {
        Ok(b) => b,
        Err(e) => return err(400, &e.to_string()).into_response(),
    };

    let user_id = body.user_id.unwrap_or_else(|| claims.map(|c| c.user_id).unwrap_or(0));
    let provider_group = body.provider_group.as_deref().unwrap_or("inherit");

    let (raw_key, mut key) = ApiKey::generate(user_id, &body.name, provider_group);

    if let Some(limit) = body.quota_limit {
        key.quota_limit = limit;
    }
    if let Some(rpm) = body.rpm_limit {
        key.rpm_limit = rpm;
    }
    if let Some(tpm) = body.tpm_limit {
        key.tpm_limit = tpm;
    }
    if let Some(ref exp_str) = body.expires_at {
        if let Ok(ts) = exp_str.parse::<i64>() {
            key.expires_at = Some(ts);
        }
    }
    if let Some(ref qg) = body.quota_group {
        key.quota_group = Some(qg.clone());
    }

    match ApiKeyDao::insert(&state.db, &key) {
        Ok(id) => ok(json!({
            "id": id,
            "key": raw_key,
            "name": key.name,
            "key_prefix": key.key_prefix,
            "provider_group": key.provider_group,
            "quota_limit": key.quota_limit,
            "quota_group": key.quota_group,
            "is_active": true,
            "created_at": key.created_at,
        })).into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

pub async fn update_key(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let mut key = match ApiKeyDao::get_by_id(&state.db, id) {
        Ok(Some(k)) => k,
        Ok(None) => return err(404, "API key not found").into_response(),
        Err(e) => return err(500, &e.to_string()).into_response(),
    };

    if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
        key.name = name.to_string();
    }
    if let Some(limit) = body.get("quota_limit").and_then(|v| v.as_i64()) {
        key.quota_limit = limit;
    }
    if let Some(rpm) = body.get("rpm_limit").and_then(|v| v.as_i64()) {
        key.rpm_limit = rpm as i32;
    }
    if let Some(tpm) = body.get("tpm_limit").and_then(|v| v.as_i64()) {
        key.tpm_limit = tpm as i32;
    }
    if let Some(active) = body.get("is_active").and_then(|v| v.as_bool()) {
        key.is_active = active;
    }
    if let Some(group) = body.get("provider_group").and_then(|v| v.as_str()) {
        key.provider_group = group.to_string();
    }
    if body.get("quota_group").is_some() {
        key.quota_group = body.get("quota_group").and_then(|v| v.as_str()).map(|s| s.to_string());
    }

    match ApiKeyDao::update(&state.db, &key) {
        Ok(_) => {
            // Invalidate auth cache so rate limit changes take effect immediately
            state.proxy_state.authenticator.invalidate(&key.key_hash).await;
            ok(json!({
                "id": key.id,
                "name": key.name,
                "key_prefix": key.key_prefix,
                "is_active": key.is_active,
                "quota_limit": key.quota_limit,
                "quota_group": key.quota_group,
            })).into_response()
        }
        Err(e) => err(400, &e.to_string()).into_response(),
    }
}

pub async fn delete_key(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    // Get key_hash before deleting for cache invalidation
    let key_hash = ApiKeyDao::get_by_id(&state.db, id)
        .ok()
        .flatten()
        .map(|k| k.key_hash);

    match ApiKeyDao::delete(&state.db, id) {
        Ok(_) => {
            if let Some(hash) = key_hash {
                state.proxy_state.authenticator.invalidate(&hash).await;
            }
            ok_void().into_response()
        }
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

// ============================================================
//  Provider Connection Test (Phase 5)
// ============================================================

/// POST /api/providers/:id/test — test connectivity to a provider
pub async fn test_provider(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
    body: Option<axum::Json<serde_json::Value>>,
) -> impl IntoResponse {
    use crate::database::dao::{ProviderModelDao, ProviderEndpointDao};
    use std::time::Instant;

    // Extract optional model from request body
    let requested_model = body.as_ref()
        .and_then(|b| b.get("model"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // 1. Load provider
    let provider = match ProviderDao::get_by_id(&state.db, id) {
        Ok(Some(p)) => p,
        Ok(None) => return err(404, "Provider not found").into_response(),
        Err(e) => return err(500, &e.to_string()).into_response(),
    };

    // 2. Query chat endpoint configuration
    let endpoint = ProviderEndpointDao::get_best_endpoint(&state.db, id, "chat", "openai")
        .ok()
        .flatten();

    let base_url = provider.base_url.trim_end_matches('/');
    let api_key = provider.api_keys().into_iter().next().unwrap_or("");
    let timeout = std::time::Duration::from_secs(10);

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let start = Instant::now();

    // Determine format: prefer endpoint's api_format, fallback to provider's auth_method hint
    let ep_format = endpoint.as_ref()
        .map(|ep| ep.api_format.as_str())
        .unwrap_or_else(|| {
            if provider.auth_method.eq_ignore_ascii_case("x-api-key") {
                "anthropic"
            } else {
                "openai"
            }
        });

    // Auth is determined by provider.auth_method, NOT by api_format.
    // A provider can use anthropic message format but still authenticate with Bearer token.
    let use_anthropic_auth = provider.auth_method.eq_ignore_ascii_case("x-api-key");

    // If no endpoint configured AND no model requested, use GET /v1/models for connectivity test
    if endpoint.is_none() && requested_model.is_none() {
        let url = format!("{}/v1/models", base_url);
        let mut req = client.get(&url);
        if use_anthropic_auth {
            req = req.header("x-api-key", api_key)
                     .header("anthropic-version", "2023-06-01");
        } else {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let result = req.send().await;
        let latency_ms = start.elapsed().as_millis() as u64;

        return match result {
            Ok(resp) => {
                let status = resp.status().as_u16();
                // Any HTTP response means connectivity is OK
                // Only 401 means the API key is invalid
                if status == 401 {
                    let error_body = resp.text().await.unwrap_or_default();
                    let error_msg = if error_body.len() > 200 {
                        format!("HTTP {} - {}", status, &error_body[..200])
                    } else {
                        format!("HTTP {} - {}", status, error_body)
                    };
                    ok(json!({
                        "success": false,
                        "latency_ms": latency_ms,
                        "status_code": status,
                        "error": error_msg,
                    })).into_response()
                } else {
                    if provider.status != crate::models::ProviderStatus::Enabled {
                        reactivate_provider(&state.db, id);
                    }
                    ok(json!({
                        "success": true,
                        "latency_ms": latency_ms,
                        "status_code": status,
                    })).into_response()
                }
            }
            Err(e) => {
                let error_msg = if e.is_timeout() {
                    "Connection timed out (10s)".to_string()
                } else if e.is_connect() {
                    format!("Connection failed: {}", e)
                } else {
                    e.to_string()
                };
                ok(json!({
                    "success": false,
                    "latency_ms": latency_ms,
                    "error": error_msg,
                })).into_response()
            }
        };
    }

    // 3. Pick a test model: use requested_model if provided, otherwise auto-select
    let test_model = requested_model.unwrap_or_else(|| {
        ProviderModelDao::list_by_provider(&state.db, id)
        .ok()
        .and_then(|models| {
            let provider_name = provider.name.to_lowercase().replace("-anthropic", "");
            let cheap_keywords = ["flash", "turbo", "mini", "lite"];
            let exclude_keywords = ["audio", "image", "video", "tts", "asr", "embedding", "rerank", "ocr", "translate", "wan2"];
            // Prefer a cheap chat model whose name matches the provider
            let mut best: Option<String> = None;
            let mut fallback_match: Option<String> = None;
            for m in &models {
                let lower = m.model_pattern.to_lowercase();
                if !lower.contains(&provider_name) {
                    continue;
                }
                if exclude_keywords.iter().any(|k| lower.contains(k)) {
                    continue;
                }
                if cheap_keywords.iter().any(|k| lower.contains(k)) {
                    best = Some(m.model_pattern.clone());
                    break;
                }
                if fallback_match.is_none() {
                    fallback_match = Some(m.model_pattern.clone());
                }
            }
            best.or(fallback_match).or_else(|| models.first().map(|m| m.model_pattern.clone()))
        })
        .unwrap_or_else(|| {
            // Infer a reasonable model name from the provider name
            let name = provider.name.to_lowercase();
            if name.contains("deepseek") {
                "deepseek-chat".to_string()
            } else if name.contains("qwen") || name.contains("tongyi") {
                "qwen-turbo".to_string()
            } else if name.contains("moonshot") || name.contains("kimi") {
                "moonshot-v1-8k".to_string()
            } else if name.contains("zhipu") || name.contains("glm") {
                "glm-4-flash".to_string()
            } else if name.contains("minimax") {
                "MiniMax-Text-01".to_string()
            } else if name.contains("anthropic") || name.contains("claude") {
                "claude-haiku-4-5-20251001".to_string()
            } else {
                "gpt-4o-mini".to_string()
            }
        })
    });

    tracing::info!("[test_provider] id={}, name={}, test_model={}", id, provider.name, test_model);

    // 4. Send chat test request
    let path = endpoint.as_ref()
        .map(|ep| ep.endpoint_path.clone())
        .unwrap_or_else(|| {
            if ep_format.eq_ignore_ascii_case("anthropic") {
                "/v1/messages".to_string()
            } else {
                "/v1/chat/completions".to_string()
            }
        });
    let url = format!("{}{}", base_url, path);

    let (body, content_type) = if ep_format.eq_ignore_ascii_case("anthropic") {
        (serde_json::json!({
            "model": test_model,
            "max_tokens": 10,
            "messages": [{"role": "user", "content": "hi"}]
        }), "application/json")
    } else {
        (serde_json::json!({
            "model": test_model,
            "max_tokens": 10,
            "messages": [{"role": "user", "content": "hi"}]
        }), "application/json")
    };

    let mut req = client.post(&url)
        .header("content-type", content_type)
        .json(&body);

    if use_anthropic_auth {
        req = req.header("x-api-key", api_key)
                 .header("anthropic-version", "2023-06-01");
    } else {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }

    let result = req.send().await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(resp) => {
            let status = resp.status().as_u16();
            if status >= 200 && status < 400 {
                // Re-enable provider if currently disabled
                if provider.status != crate::models::ProviderStatus::Enabled {
                    reactivate_provider(&state.db, id);
                }
                ok(json!({
                    "success": true,
                    "latency_ms": latency_ms,
                    "status_code": status,
                })).into_response()
            } else {
                let error_body = resp.text().await.unwrap_or_default();
                // 403 with "access denied" means auth succeeded but model not enabled
                // Treat as connectivity OK since the API key is valid
                if status == 403 && (error_body.contains("access_denied") || error_body.contains("AccessDenied")) {
                    let latency_ms = start.elapsed().as_millis() as u64;
                    if provider.status != crate::models::ProviderStatus::Enabled {
                        reactivate_provider(&state.db, id);
                    }
                    return ok(json!({
                        "success": true,
                        "latency_ms": latency_ms,
                        "status_code": status,
                        "note": format!("Model '{}' not enabled in workspace, but API key is valid", test_model),
                    })).into_response();
                }
                let error_msg = if error_body.len() > 200 {
                    format!("HTTP {} - {}", status, &error_body[..200])
                } else {
                    format!("HTTP {} - {}", status, error_body)
                };
                ok(json!({
                    "success": false,
                    "latency_ms": latency_ms,
                    "status_code": status,
                    "error": error_msg,
                })).into_response()
            }
        }
        Err(e) => {
            let error_msg = if e.is_timeout() {
                "Connection timed out (10s)".to_string()
            } else if e.is_connect() {
                format!("Connection failed: {}", e)
            } else {
                e.to_string()
            };
            ok(json!({
                "success": false,
                "latency_ms": latency_ms,
                "error": error_msg,
            })).into_response()
        }
    }
}

/// Re-activate a disabled provider after successful connectivity test
fn reactivate_provider(db: &Database, provider_id: i64) {
    use crate::models::ProviderStatus;
    tracing::info!("[Test] Re-enabling provider #{} after successful test", provider_id);
    let now = chrono::Utc::now().timestamp_millis();
    #[cfg(feature = "sqlite")]
    {
        let _ = db.execute(
            "UPDATE providers SET status = ?1, updated_at = ?2 WHERE id = ?3 AND status = ?4",
            rusqlite::params![ProviderStatus::Enabled as i32, now, provider_id, ProviderStatus::AutoDisabled as i32],
        );
    }
    #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
    {
        let _ = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("UPDATE providers SET status = ?, updated_at = ? WHERE id = ? AND status = ?")
                    .bind(ProviderStatus::Enabled as i32).bind(now).bind(provider_id).bind(ProviderStatus::AutoDisabled as i32)
                    .execute(&db.pool).await
            })
        });
    }
}

// ============================================================
//  Provider Type Presets
// ============================================================

/// Fetch available models from the upstream provider (dynamic discovery)
pub async fn fetch_provider_models(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {

    // 1. Load provider
    let provider = match ProviderDao::get_by_id(&state.db, id) {
        Ok(Some(p)) => p,
        Ok(None) => return err(404, "Provider not found").into_response(),
        Err(e) => return err(500, &e.to_string()).into_response(),
    };

    // 2. Query models endpoint configuration
    use crate::database::dao::ProviderEndpointDao;
    let endpoint = ProviderEndpointDao::get_by_provider_and_type(&state.db, id, "models")
        .ok()
        .flatten();

    let base_url = provider.base_url.trim_end_matches('/');
    let api_key = provider.api_keys().into_iter().next().unwrap_or("");
    let timeout = std::time::Duration::from_secs(15);

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    // 3. Fetch models using endpoint config
    let path = endpoint.as_ref()
        .map(|ep| ep.endpoint_path.clone())
        .unwrap_or_else(|| "/v1/models".to_string());
    let url = format!("{}{}", base_url, path);
    let resp = client.get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await;
    let result = match resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.unwrap_or(Value::Null);
            let models: Vec<String> = body.get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter()
                    .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(String::from))
                    .collect())
                .unwrap_or_default();
            Ok(models)
        }
        Ok(r) => {
            let status = r.status().as_u16();
            let body = r.text().await.unwrap_or_default();
            Err(format!("HTTP {} - {}", status, body.chars().take(200).collect::<String>()))
        }
        Err(e) => Err(e.to_string()),
    };

    // 3. Return response
    match result {
        Ok(models) => ok(json!({ "models": models, "count": models.len() })).into_response(),
        Err(e) => ok(json!({ "models": [], "count": 0, "error": e })).into_response(),
    }
}

/// Fetch models from an upstream provider without saving it first.
/// POST body: { "base_url": "...", "api_key": "...", "api_format": "openai"|"anthropic"|"google"|"openai_responses" }
pub async fn fetch_models_adhoc(
    State(_state): State<AdminState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    use crate::models::ApiFormat;

    let base_url = match body.get("base_url").and_then(|v| v.as_str()) {
        Some(u) => u.trim_end_matches('/'),
        None => return err(400, "base_url is required").into_response(),
    };
    let api_key = body.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
    let api_format_str = body.get("api_format").and_then(|v| v.as_str()).unwrap_or("openai");
    let _provider_type = body.get("provider_type").and_then(|v| v.as_str()).unwrap_or("");

    let api_format: ApiFormat = api_format_str.parse().unwrap_or(ApiFormat::OpenAI);

    let timeout = std::time::Duration::from_secs(15);
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let result = match api_format {
        ApiFormat::OpenAI | ApiFormat::OpenAIResponses | ApiFormat::Passthrough => {
            let url = format!("{}/v1/models", base_url);
            let resp = client.get(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    let body: serde_json::Value = r.json().await.unwrap_or(Value::Null);
                    let models: Vec<String> = body.get("data")
                        .and_then(|d| d.as_array())
                        .map(|arr| arr.iter()
                            .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(String::from))
                            .collect())
                        .unwrap_or_default();
                    Ok(models)
                }
                Ok(r) => {
                    let status = r.status().as_u16();
                    let text = r.text().await.unwrap_or_default();
                    Err(format!("HTTP {} - {}", status, text.chars().take(200).collect::<String>()))
                }
                Err(e) => Err(e.to_string()),
            }
        }
        ApiFormat::Anthropic => {
            // NOTE: In endpoint-driven architecture, models are no longer stored in presets
            // They should be fetched from models_metadata table if needed
            let models: Vec<String> = Vec::new();
            Ok(models)
        }
        ApiFormat::Google => {
            let url = format!("{}/v1beta/models?key={}", base_url, api_key);
            let resp = client.get(&url).send().await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    let body: serde_json::Value = r.json().await.unwrap_or(Value::Null);
                    let models: Vec<String> = body.get("models")
                        .and_then(|d| d.as_array())
                        .map(|arr| arr.iter()
                            .filter_map(|m| m.get("name").and_then(|n| n.as_str())
                                .map(|n| n.strip_prefix("models/").unwrap_or(n).to_string()))
                            .collect())
                        .unwrap_or_default();
                    Ok(models)
                }
                Ok(r) => {
                    let status = r.status().as_u16();
                    let text = r.text().await.unwrap_or_default();
                    Err(format!("HTTP {} - {}", status, text.chars().take(200).collect::<String>()))
                }
                Err(e) => Err(e.to_string()),
            }
        }
    };

    match result {
        Ok(models) => ok(json!({ "models": models, "count": models.len() })).into_response(),
        Err(e) => ok(json!({ "models": [], "count": 0, "error": e })).into_response(),
    }
}

pub async fn list_provider_presets(
    State(state): State<AdminState>,
) -> impl IntoResponse {
    match ProviderPresetDao::list_all_full(&state.db) {
        Ok(presets) => ok(presets).into_response(),
        Err(e) => err(500, &format!("Failed to load presets: {}", e)).into_response(),
    }
}

pub async fn get_provider_preset(
    State(state): State<AdminState>,
    Path(provider_type): Path<String>,
) -> impl IntoResponse {
    match ProviderPresetDao::get_full(&state.db, &provider_type) {
        Ok(Some(preset)) => ok(preset).into_response(),
        Ok(None) => err(404, "Provider type preset not found").into_response(),
        Err(e) => err(500, &format!("Failed to load preset: {}", e)).into_response(),
    }
}

/// PUT /api/provider-presets/:type — upsert a preset (admin only)
pub async fn upsert_provider_preset(
    State(state): State<AdminState>,
    Path(preset_key): Path<String>,
    Json(body): Json<ProviderPresetFull>,
) -> impl IntoResponse {
    // Validate that the preset_key in path matches body
    if body.preset_key != preset_key {
        return err(400, "Preset key in path does not match body").into_response();
    }

    // Convert ProviderPresetFull to ProviderPreset for insertion
    let preset = crate::models::ProviderPreset {
        id: 0, // Will be set by DB
        preset_key: body.preset_key.clone(),
        display_name: body.display_name.clone(),
        base_url: body.base_url.clone(),
        auth_method: body.auth_method.clone(),
        endpoints: body.endpoints.clone(),
        category: "llm".to_string(),
        description: body.description.clone(),
        sort_order: 0,
        is_active: true,
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
    };

    // Check if preset exists, then insert or update
    match ProviderPresetDao::get_by_key(&state.db, &preset_key) {
        Ok(Some(existing)) => {
            let updated = crate::models::ProviderPreset {
                id: existing.id,
                sort_order: existing.sort_order,
                created_at: existing.created_at,
                ..preset
            };
            match ProviderPresetDao::update(&state.db, &updated) {
                Ok(_) => {
                    tracing::info!("Provider preset updated: {}", preset_key);
                    // Presets are read from DB on-demand, no cache to invalidate
                    ok_void().into_response()
                }
                Err(e) => err(500, &format!("Failed to update preset: {}", e)).into_response(),
            }
        }
        Ok(None) => {
            match ProviderPresetDao::insert(&state.db, &preset) {
                Ok(_) => {
                    tracing::info!("Provider preset inserted: {}", preset_key);
                    // Presets are read from DB on-demand, no cache to invalidate
                    ok_void().into_response()
                }
                Err(e) => err(500, &format!("Failed to insert preset: {}", e)).into_response(),
            }
        }
        Err(e) => err(500, &format!("Failed to check existing preset: {}", e)).into_response(),
    }
}

/// DELETE /api/provider-presets/:key — delete a preset
pub async fn delete_provider_preset(
    State(state): State<AdminState>,
    Path(preset_key): Path<String>,
) -> impl IntoResponse {
    // Find the preset first
    match ProviderPresetDao::get_by_key(&state.db, &preset_key) {
        Ok(Some(preset)) => {
            match ProviderPresetDao::delete(&state.db, preset.id) {
                Ok(_) => {
                    tracing::info!("Provider preset deleted: {}", preset_key);
                    // Presets are read from DB on-demand, no cache to invalidate
                    ok_void().into_response()
                }
                Err(e) => err(500, &format!("Failed to delete preset: {}", e)).into_response(),
            }
        }
        Ok(None) => err(404, "Preset not found").into_response(),
        Err(e) => err(500, &format!("Failed to find preset: {}", e)).into_response(),
    }
}

// ============================================================
//  Provider Endpoints CRUD
// ============================================================

/// GET /api/providers/:id/endpoints — list all endpoints for a provider
pub async fn list_provider_endpoints(
    State(state): State<AdminState>,
    Path(provider_id): Path<i64>,
) -> impl IntoResponse {
    use crate::database::dao::ProviderEndpointDao;
    match ProviderEndpointDao::list_by_provider(&state.db, provider_id) {
        Ok(endpoints) => ok(endpoints).into_response(),
        Err(e) => err(500, &format!("Failed to list endpoints: {}", e)).into_response(),
    }
}

/// POST /api/providers/:id/endpoints — create an endpoint for a provider
pub async fn create_provider_endpoint(
    State(state): State<AdminState>,
    Path(provider_id): Path<i64>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    use crate::database::dao::ProviderEndpointDao;
    use crate::models::provider_endpoint::ProviderEndpoint;

    let endpoint_type = body.get("endpoint_type").and_then(|v| v.as_str()).unwrap_or("chat");
    let api_format = body.get("api_format").and_then(|v| v.as_str())
        .or_else(|| body.get("format").and_then(|v| v.as_str()))
        .unwrap_or("openai");
    let endpoint_path = body.get("endpoint_path").and_then(|v| v.as_str())
        .or_else(|| body.get("path").and_then(|v| v.as_str()))
        .unwrap_or("/v1/chat/completions");

    let endpoint = ProviderEndpoint::new(provider_id, endpoint_type, api_format, endpoint_path);

    match ProviderEndpointDao::insert(&state.db, &endpoint) {
        Ok(_) => ok(json!({"message": "Endpoint created"})).into_response(),
        Err(e) => err(500, &format!("Failed to create endpoint: {}", e)).into_response(),
    }
}

/// PUT /api/provider-endpoints/:id — update an endpoint
pub async fn update_provider_endpoint(
    State(state): State<AdminState>,
    Path(endpoint_id): Path<i64>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    use crate::database::dao::ProviderEndpointDao;

    match ProviderEndpointDao::update_from_json(&state.db, endpoint_id, &body) {
        Ok(_) => ok(json!({"message": "Endpoint updated"})).into_response(),
        Err(e) => err(500, &format!("Failed to update endpoint: {}", e)).into_response(),
    }
}

/// DELETE /api/provider-endpoints/:id — delete an endpoint
pub async fn delete_provider_endpoint(
    State(state): State<AdminState>,
    Path(endpoint_id): Path<i64>,
) -> impl IntoResponse {
    use crate::database::dao::ProviderEndpointDao;
    match ProviderEndpointDao::delete_by_id(&state.db, endpoint_id) {
        Ok(_) => ok(json!({"message": "Endpoint deleted"})).into_response(),
        Err(e) => err(500, &format!("Failed to delete endpoint: {}", e)).into_response(),
    }
}

// ============================================================
//  Data Migration (export/import org + users + api_keys)
// ============================================================

/// GET /api/migration/export — export complete migration package
pub async fn export_migration(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let claims = match request.extensions().get::<Claims>().cloned() {
        Some(c) => c,
        None => return err(401, "Unauthorized").into_response(),
    };
    if !permission::is_org_admin(&claims) {
        return err(403, "Only org_admin can export migration data").into_response();
    }

    let orgs = match OrganizationDao::list_all(&state.db) {
        Ok(v) => v,
        Err(e) => return err(500, &e.to_string()).into_response(),
    };

    let mut org_list: Vec<Value> = Vec::new();
    for org in &orgs {
        // Users for this org, with their api_keys
        let users = UserDao::list_by_org(&state.db, org.id).unwrap_or_default();
        let mut users_json: Vec<Value> = Vec::new();
        for u in &users {
            // API keys for this user
            let keys = ApiKeyDao::list_by_user(&state.db, u.id).unwrap_or_default();
            let keys_json: Vec<Value> = keys.iter().map(|k| {
                // Export the plaintext key so the target instance can re-encrypt with its own master key
                let plaintext_key = k.encrypted_key.as_ref()
                    .and_then(|enc| crate::crypto::decrypt_if_encrypted(enc).ok());
                json!({
                    "name": k.name,
                    "key_hash": k.key_hash,
                    "key_prefix": k.key_prefix,
                    "key_suffix": k.key_suffix,
                    "plaintext_key": plaintext_key,
                    "provider_group": k.provider_group,
                    "quota_limit": k.quota_limit,
                    "quota_group": k.quota_group,
                    "rpm_limit": k.rpm_limit,
                    "tpm_limit": k.tpm_limit,
                    "is_active": k.is_active,
                    "expires_at": k.expires_at,
                })
            }).collect();

            users_json.push(json!({
                "username": u.username,
                "display_name": u.display_name,
                "email": u.email,
                "role": u.role.to_string(),
                "provider_group": u.provider_group,
                "is_active": u.is_active,
                "password_hash": u.password_hash,
                "api_keys": keys_json,
            }));
        }

        org_list.push(json!({
            "name": org.name,
            "display_name": org.display_name,
            "is_active": org.is_active,
            "settings": org.settings,
            "users": users_json,
        }));
    }

    ok(json!({
        "version": 1,
        "exported_at": chrono::Utc::now().timestamp_millis(),
        "organizations": org_list,
    })).into_response()
}

/// POST /api/migration/import — import complete migration package
pub async fn import_migration(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let claims = match request.extensions().get::<Claims>().cloned() {
        Some(c) => c,
        None => return err(401, "Unauthorized").into_response(),
    };
    if !permission::is_org_admin(&claims) {
        return err(403, "Only org_admin can import migration data").into_response();
    }

    let body: Value = match axum::body::to_bytes(request.into_body(), 10 * 1024 * 1024).await {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => return err(400, &format!("Invalid JSON: {}", e)).into_response(),
        },
        Err(_) => return err(400, "Request body too large or invalid").into_response(),
    };

    let orgs_arr = match body.get("organizations").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return err(400, "Missing 'organizations' array").into_response(),
    };

    let mut stats = json!({
        "orgs": { "created": 0, "updated": 0 },
        "users": { "created": 0, "updated": 0 },
        "keys": { "created": 0, "skipped": 0 },
    });

    for org_item in orgs_arr {
        let org_name = org_item.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if org_name.is_empty() { continue; }

        // Upsert organization by name
        let existing_orgs = OrganizationDao::list_all(&state.db).unwrap_or_default();
        let org_id = if let Some(existing) = existing_orgs.iter().find(|o| o.name == org_name) {
            // Update existing org
            let mut org = existing.clone();
            if let Some(dn) = org_item.get("display_name").and_then(|v| v.as_str()) {
                org.display_name = Some(dn.to_string());
            }
            if let Some(active) = org_item.get("is_active").and_then(|v| v.as_bool()) {
                org.is_active = active;
            }
            if let Some(settings) = org_item.get("settings") {
                org.settings = if settings.is_null() { None } else { Some(settings.clone()) };
            }
            let _ = OrganizationDao::update(&state.db, &org);
            inc(&mut stats, "orgs", "updated");
            existing.id
        } else {
            let mut org = Organization::new(org_name.to_string());
            org.display_name = org_item.get("display_name").and_then(|v| v.as_str()).map(|s| s.to_string());
            if let Some(active) = org_item.get("is_active").and_then(|v| v.as_bool()) {
                org.is_active = active;
            }
            if let Some(settings) = org_item.get("settings") {
                org.settings = if settings.is_null() { None } else { Some(settings.clone()) };
            }
            match OrganizationDao::insert(&state.db, &org) {
                Ok(id) => { inc(&mut stats, "orgs", "created"); id }
                Err(_) => continue,
            }
        };

        // Import users
        if let Some(users_arr) = org_item.get("users").and_then(|v| v.as_array()) {
            for u_item in users_arr {
                let username = u_item.get("username").and_then(|v| v.as_str()).unwrap_or("");
                if username.is_empty() { continue; }

                let password_hash = u_item.get("password_hash").and_then(|v| v.as_str()).unwrap_or("").to_string();

                let user_id = if let Ok(Some(existing)) = UserDao::get_by_username(&state.db, org_id, username) {
                    // Update existing user
                    let mut user = existing.clone();
                    if let Some(dn) = u_item.get("display_name").and_then(|v| v.as_str()) {
                        user.display_name = Some(dn.to_string());
                    }
                    if let Some(email) = u_item.get("email").and_then(|v| v.as_str()) {
                        user.email = Some(email.to_string());
                    }
                    if let Some(role_str) = u_item.get("role").and_then(|v| v.as_str()) {
                        if let Ok(role) = role_str.parse::<UserRole>() {
                            user.role = role;
                        }
                    }
                    if let Some(group) = u_item.get("provider_group").and_then(|v| v.as_str()) {
                        user.provider_group = group.to_string();
                    }
                    if let Some(active) = u_item.get("is_active").and_then(|v| v.as_bool()) {
                        user.is_active = active;
                    }
                    let _ = UserDao::update(&state.db, &user);
                    inc(&mut stats, "users", "updated");
                    existing.id
                } else {
                    let mut user = User::new(org_id, username.to_string(), password_hash);
                    user.display_name = u_item.get("display_name").and_then(|v| v.as_str()).map(|s| s.to_string());
                    user.email = u_item.get("email").and_then(|v| v.as_str()).map(|s| s.to_string());
                    if let Some(role_str) = u_item.get("role").and_then(|v| v.as_str()) {
                        if let Ok(role) = role_str.parse::<UserRole>() {
                            user.role = role;
                        }
                    }
                    if let Some(group) = u_item.get("provider_group").and_then(|v| v.as_str()) {
                        user.provider_group = group.to_string();
                    }
                    if let Some(active) = u_item.get("is_active").and_then(|v| v.as_bool()) {
                        user.is_active = active;
                    }
                    match UserDao::insert(&state.db, &user) {
                        Ok(id) => { inc(&mut stats, "users", "created"); id }
                        Err(_) => continue,
                    }
                };

                // Import API keys for this user
                if let Some(keys_arr) = u_item.get("api_keys").and_then(|v| v.as_array()) {
                    for k_item in keys_arr {
                        let key_hash = k_item.get("key_hash").and_then(|v| v.as_str()).unwrap_or("");
                        if key_hash.is_empty() { continue; }

                        // Skip if key_hash already exists
                        if let Ok(Some(_)) = ApiKeyDao::get_by_hash(&state.db, key_hash) {
                            inc(&mut stats, "keys", "skipped");
                            continue;
                        }

                        let name = k_item.get("name").and_then(|v| v.as_str()).unwrap_or("imported-key").to_string();
                        let key_prefix = k_item.get("key_prefix").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let key_suffix = k_item.get("key_suffix").and_then(|v| v.as_str()).unwrap_or("").to_string();

                        let mut key = ApiKey::new(user_id, name, key_hash.to_string(), key_prefix, key_suffix);
                        // Re-encrypt plaintext_key with this instance's master key
                        key.encrypted_key = k_item.get("plaintext_key").and_then(|v| v.as_str())
                            .and_then(|plain| crate::crypto::encrypt(plain).ok())
                            // Fallback: legacy format with pre-encrypted key
                            .or_else(|| k_item.get("encrypted_key").and_then(|v| v.as_str()).map(|s| s.to_string()));
                        if let Some(group) = k_item.get("provider_group").and_then(|v| v.as_str()) {
                            key.provider_group = group.to_string();
                        }
                        if let Some(limit) = k_item.get("quota_limit").and_then(|v| v.as_i64()) {
                            key.quota_limit = limit;
                        }
                        if let Some(qg) = k_item.get("quota_group").and_then(|v| v.as_str()) {
                            key.quota_group = Some(qg.to_string());
                        }
                        if let Some(rpm) = k_item.get("rpm_limit").and_then(|v| v.as_i64()) {
                            key.rpm_limit = rpm as i32;
                        }
                        if let Some(tpm) = k_item.get("tpm_limit").and_then(|v| v.as_i64()) {
                            key.tpm_limit = tpm as i32;
                        }
                        if let Some(active) = k_item.get("is_active").and_then(|v| v.as_bool()) {
                            key.is_active = active;
                        }
                        if let Some(exp) = k_item.get("expires_at").and_then(|v| v.as_i64()) {
                            key.expires_at = Some(exp);
                        }

                        match ApiKeyDao::insert(&state.db, &key) {
                            Ok(_) => inc(&mut stats, "keys", "created"),
                            Err(_) => inc(&mut stats, "keys", "skipped"),
                        }
                    }
                }
            }
        }
    }

    ok(stats).into_response()
}

/// Helper: increment a counter in the stats JSON
fn inc(stats: &mut Value, section: &str, field: &str) {
    if let Some(obj) = stats.get_mut(section).and_then(|v| v.as_object_mut()) {
        if let Some(val) = obj.get_mut(field) {
            *val = json!(val.as_i64().unwrap_or(0) + 1);
        }
    }
}