// Admin service module

pub mod auth;
pub mod server;
pub mod handlers;
pub mod handlers_models;
pub mod handlers_search;
pub mod handlers_settings;
pub mod middleware;
pub mod permission;

pub use server::AdminServer;
