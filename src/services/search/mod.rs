//! Web Search Gateway service — transparent proxy mode
//!
//! Provides `/v1/search/*` routes that authenticate the user,
//! select a search-category provider, inject its API key,
//! and transparently proxy request/response to the upstream search API.

pub mod handler;

pub use handler::{SearchState, search_handler};
