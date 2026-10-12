// Admin API handlers — System Settings management

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::Value;
use crate::database::dao::SystemSettingsDao;
use crate::models::SystemSetting;
use super::server::AdminState;
use super::handlers::{ok, ok_void, err};

// ============================================================
//  System Settings
// ============================================================

#[derive(Deserialize)]
pub struct UpsertSettingRequest {
    pub value: Value,
    pub description: Option<String>,
}

/// GET /api/settings — list all settings
pub async fn list_settings(
    State(state): State<AdminState>,
) -> impl IntoResponse {
    match SystemSettingsDao::list_all(&state.db) {
        Ok(settings) => ok(settings).into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

/// GET /api/settings/:key — get single setting
pub async fn get_setting(
    State(state): State<AdminState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    match SystemSettingsDao::get(&state.db, &key) {
        Ok(Some(s)) => ok(s).into_response(),
        Ok(None) => err(404, "Setting not found").into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}

/// PUT /api/settings/:key — upsert setting
pub async fn upsert_setting(
    State(state): State<AdminState>,
    Path(key): Path<String>,
    Json(body): Json<UpsertSettingRequest>,
) -> impl IntoResponse {
    let mut setting = SystemSetting::new(key.clone(), body.value);
    setting.description = body.description;

    match SystemSettingsDao::set(&state.db, &setting) {
        Ok(_) => {
            match SystemSettingsDao::get(&state.db, &key) {
                Ok(Some(s)) => ok(s).into_response(),
                _ => ok_void().into_response(),
            }
        }
        Err(e) => err(400, &e.to_string()).into_response(),
    }
}

/// DELETE /api/settings/:key — delete setting
pub async fn delete_setting(
    State(state): State<AdminState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    match SystemSettingsDao::delete(&state.db, &key) {
        Ok(_) => ok_void().into_response(),
        Err(e) => err(500, &e.to_string()).into_response(),
    }
}
