//! In-process path template cache.
//! Loaded from `provider_type_presets` table at startup, refreshed on cache invalidation.
//! This is NOT stored in Redis — it's static config data that changes rarely.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use crate::models::PathTemplates;
use crate::services::proxy::relay_mode::RelayMode;

/// Global path template cache (process-local, not Redis)
fn cache() -> &'static RwLock<HashMap<String, PathTemplates>> {
    static CACHE: OnceLock<RwLock<HashMap<String, PathTemplates>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Load all path templates from presets into the in-memory cache
pub fn load_from_presets(presets: &[(String, PathTemplates)]) {
    let mut c = cache().write().unwrap();
    c.clear();
    for (provider_type, templates) in presets {
        c.insert(provider_type.clone(), templates.clone());
    }
    tracing::info!("Path template cache loaded: {} provider types", c.len());
}

/// Look up a path template for a given provider_type + relay_mode.
/// Returns None if no template is configured (caller should fallback to adapter code).
pub fn resolve_path(provider_type: &str, relay_mode: &RelayMode) -> Option<String> {
    let c = cache().read().unwrap();
    let templates = c.get(provider_type)?;

    let path = match relay_mode {
        RelayMode::ChatCompletions | RelayMode::Messages => templates.chat.as_deref(),
        RelayMode::Completions => templates.completions.as_deref(),
        RelayMode::Embeddings => templates.embeddings.as_deref(),
        RelayMode::ImagesGenerations | RelayMode::ImagesEdits => templates.images.as_deref(),
        RelayMode::AudioSpeech => templates.audio_speech.as_deref(),
        RelayMode::AudioTranscriptions => templates.audio_transcriptions.as_deref(),
        RelayMode::Moderations => templates.moderations.as_deref(),
        RelayMode::Rerank => templates.rerank.as_deref(),
        RelayMode::Responses => templates.responses.as_deref(),
        RelayMode::Passthrough(_) => None,
    };

    path.map(|s| s.to_string())
}

/// Clear the cache (for testing or shutdown)
pub fn clear() {
    cache().write().unwrap().clear();
}
