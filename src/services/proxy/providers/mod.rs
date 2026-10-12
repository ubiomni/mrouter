pub mod adapter;
pub mod auth;
pub mod custom;
// pub mod template; // Temporarily disabled during refactoring

pub use adapter::ProviderAdapter;

use crate::models::Provider;
use self::custom::CustomAdapter;

/// Get the appropriate adapter for a provider.
/// TODO: Reimplement using endpoint-driven architecture
pub fn get_adapter(_provider: &Provider) -> Box<dyn ProviderAdapter> {
    Box::new(CustomAdapter)
}
