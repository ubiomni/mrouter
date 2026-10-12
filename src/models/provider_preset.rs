// Provider Preset 数据模型 (Endpoint-driven architecture)

use serde::{Deserialize, Serialize};

/// Preset endpoint definition (stored in JSON)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetEndpoint {
    pub endpoint_type: String,
    pub path: String,
    pub format: String,
}

/// Provider preset (template for creating providers) - NEW structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPreset {
    pub id: i64,
    pub preset_key: String,
    pub display_name: Option<String>,
    pub base_url: Option<String>,
    pub auth_method: String,
    pub endpoints: Vec<PresetEndpoint>,
    pub category: String,
    pub description: Option<String>,
    pub sort_order: i32,
    pub is_active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Aggregated preset for API response (matches frontend ProviderPreset interface)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPresetFull {
    pub preset_key: String,
    pub display_name: Option<String>,
    pub base_url: Option<String>,
    pub auth_method: String,
    pub endpoints: Vec<PresetEndpoint>,
    pub category: String,
    pub description: Option<String>,
}

// ============================================================
// DEPRECATED: Legacy models for backward compatibility
// These will be removed in a future version
// ============================================================

/// DEPRECATED: Use ProviderPreset instead
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated(note = "Use ProviderPreset with endpoints JSON instead")]
pub struct ProviderTypePresetRow {
    pub id: i64,
    pub provider_type: String,
    pub display_name: String,
    pub api_format: String,
    pub auth_method: String,
    pub base_url: String,
    pub features: Option<String>,
    pub description: Option<String>,
    pub sort_order: i32,
    pub is_active: bool,
    pub path_chat: Option<String>,
    pub path_completions: Option<String>,
    pub path_embeddings: Option<String>,
    pub path_images: Option<String>,
    pub path_audio_speech: Option<String>,
    pub path_audio_transcriptions: Option<String>,
    pub path_moderations: Option<String>,
    pub path_rerank: Option<String>,
    pub path_responses: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// DEPRECATED: Use PresetEndpoint instead
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[deprecated(note = "Use Vec<PresetEndpoint> instead")]
pub struct PathTemplates {
    pub chat: Option<String>,
    pub completions: Option<String>,
    pub embeddings: Option<String>,
    pub images: Option<String>,
    pub audio_speech: Option<String>,
    pub audio_transcriptions: Option<String>,
    pub moderations: Option<String>,
    pub rerank: Option<String>,
    pub responses: Option<String>,
}

/// DEPRECATED: Regions removed in endpoint-driven architecture
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated(note = "Regions are no longer used")]
pub struct ProviderTypeRegionRow {
    pub id: i64,
    pub provider_type: String,
    pub region_name: String,
    pub base_url: String,
    pub display_name: Option<String>,
    pub sort_order: i32,
}

/// DEPRECATED: Models removed in endpoint-driven architecture
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated(note = "Models metadata moved to separate table")]
pub struct ProviderTypeModelRow {
    pub id: i64,
    pub provider_type: String,
    pub model_id: String,
    pub display_name: Option<String>,
    pub supports_vision: bool,
    pub supports_thinking: bool,
    pub sort_order: i32,
}

/// DEPRECATED: Use ProviderPresetFull instead
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated(note = "Use ProviderPresetFull with endpoints instead")]
pub struct ProviderPresetRegion {
    pub name: String,
    pub base_url: String,
    pub display_name: Option<String>,
}

/// DEPRECATED: Use model_metadata table instead
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated(note = "Use models_metadata table instead")]
pub struct ProviderPresetModel {
    pub model_id: String,
    pub display_name: Option<String>,
    pub supports_vision: bool,
    pub supports_thinking: bool,
}
