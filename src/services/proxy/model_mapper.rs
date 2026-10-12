// 模型映射模块
//
// In the gateway rewrite, model mappings live in the provider_models table.
// This module is kept as a minimal stub for format compatibility.

use crate::models::Provider;
use serde_json::Value;

/// 对请求体应用模型映射
///
/// In the new design, model mapping is handled by ProviderRouter.
/// This stub passes body through unchanged.
pub fn apply_model_mapping(
    body: Value,
    _provider: &Provider,
) -> (Value, Option<String>, Option<String>) {
    let original_model = body.get("model").and_then(|m| m.as_str()).map(String::from);
    (body, original_model, None)
}
