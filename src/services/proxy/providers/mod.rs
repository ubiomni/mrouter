pub mod adapter;
pub mod auth;
pub mod anthropic;
pub mod openai;
pub mod google;
pub mod custom;

pub use adapter::ProviderAdapter;

use crate::models::{Provider, ProviderType};
use self::anthropic::AnthropicAdapter;
use self::openai::OpenAIAdapter;
use self::google::GoogleAdapter;
use self::custom::CustomAdapter;

/// Get the appropriate adapter for a provider
pub fn get_adapter(provider: &Provider) -> Box<dyn ProviderAdapter> {
    match provider.provider_type {
        ProviderType::Anthropic => Box::new(AnthropicAdapter),
        ProviderType::Google | ProviderType::GoogleVertexAI => Box::new(GoogleAdapter),
        ProviderType::Custom => Box::new(CustomAdapter),
        // All other providers use OpenAI-compatible API
        _ => Box::new(OpenAIAdapter),
    }
}
