//! In-process format header cache.
//! Loaded from `api_format_headers` table at startup.
//! Maps api_format → list of header rules to apply on outgoing requests.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use axum::http::HeaderMap;

/// Merge mode determines how a format header interacts with existing client headers
#[derive(Debug, Clone, PartialEq)]
pub enum MergeMode {
    /// Unconditionally set the header (overrides client value)
    Set,
    /// Append to existing value (comma-separated), or create if absent
    Append,
    /// Ensure the value is contained; append if not, create if absent
    EnsureContains,
    /// Forward client header if present, otherwise use default value
    Forward,
}

impl MergeMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "append" => Self::Append,
            "ensure_contains" => Self::EnsureContains,
            "forward" => Self::Forward,
            _ => Self::Set,
        }
    }
}

/// A single header rule for a given api_format
#[derive(Debug, Clone)]
pub struct FormatHeaderRule {
    pub header_name: String,
    pub header_value: String,
    pub merge_mode: MergeMode,
}

/// Global format header cache (process-local, not Redis)
fn cache() -> &'static RwLock<HashMap<String, Vec<FormatHeaderRule>>> {
    static CACHE: OnceLock<RwLock<HashMap<String, Vec<FormatHeaderRule>>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Load all format header rules from DB rows into the in-memory cache.
/// Input: Vec of (api_format, header_name, header_value, merge_mode)
pub fn load_from_rows(rows: &[(String, String, String, String)]) {
    let mut c = cache().write().unwrap();
    c.clear();
    for (api_format, header_name, header_value, merge_mode) in rows {
        let rule = FormatHeaderRule {
            header_name: header_name.clone(),
            header_value: header_value.clone(),
            merge_mode: MergeMode::from_str(merge_mode),
        };
        c.entry(api_format.clone()).or_default().push(rule);
    }
    tracing::info!("Format header cache loaded: {} api_formats", c.len());
}

/// Get the list of header names managed by a given api_format.
/// Used by the header-forwarding loop to skip these headers (avoid duplicates).
pub fn managed_headers_for(api_format: &str) -> Vec<String> {
    let c = cache().read().unwrap();
    match c.get(api_format) {
        Some(rules) => rules.iter().map(|r| r.header_name.clone()).collect(),
        None => vec![],
    }
}

/// Apply format headers to a request builder based on the provider's api_format.
/// Returns the modified request builder.
pub fn apply(
    mut request: reqwest::RequestBuilder,
    api_format: &str,
    client_headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    let c = cache().read().unwrap();
    let rules = match c.get(api_format) {
        Some(r) => r.clone(),
        None => return request,
    };
    drop(c); // Release lock before doing work

    for rule in &rules {
        match rule.merge_mode {
            MergeMode::Set => {
                request = request.header(&rule.header_name, &rule.header_value);
            }
            MergeMode::Forward => {
                if let Some(val) = client_headers.get(&rule.header_name).and_then(|v| v.to_str().ok()) {
                    request = request.header(&rule.header_name, val);
                } else {
                    request = request.header(&rule.header_name, &rule.header_value);
                }
            }
            MergeMode::EnsureContains => {
                let existing = client_headers
                    .get(&rule.header_name)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                if existing.contains(&rule.header_value) {
                    request = request.header(&rule.header_name, existing);
                } else if existing.is_empty() {
                    request = request.header(&rule.header_name, &rule.header_value);
                } else {
                    request = request.header(&rule.header_name, format!("{},{}", existing, rule.header_value));
                }
            }
            MergeMode::Append => {
                let existing = client_headers
                    .get(&rule.header_name)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                if existing.is_empty() {
                    request = request.header(&rule.header_name, &rule.header_value);
                } else {
                    request = request.header(&rule.header_name, format!("{},{}", existing, rule.header_value));
                }
            }
        }
    }

    request
}

/// Clear the cache (for testing or shutdown)
pub fn clear() {
    cache().write().unwrap().clear();
}
