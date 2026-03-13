//! 模型降级和排名逻辑

use crate::config::AppConfig;
use crate::models::Provider;

/// 获取模型降级链
///
/// 根据配置和可用的 providers，返回降级模型列表
pub fn get_model_fallback_chain(
    config: &AppConfig,
    original_model: &str,
    providers: &[Provider],
) -> Vec<String> {
    // 检查是否启用了模型降级
    if !config.model_fallback.enabled {
        return vec![];
    }

    // 获取降级链配置
    let fallback_chain = config
        .model_fallback
        .fallback_chains
        .get(original_model)
        .cloned()
        .unwrap_or_default();

    // 过滤出 providers 支持的模型
    fallback_chain
        .into_iter()
        .filter(|model| providers.iter().any(|p| p.supports_model(model)))
        .collect()
}

/// 模型排名（用于排序）
///
/// 返回值越大，模型越强
pub fn model_rank(model: &str) -> i32 {
    if model.contains("opus") || model.contains("o1") {
        100
    } else if model.contains("sonnet-4") || model.contains("gpt-4o") {
        90
    } else if model.contains("sonnet-3.7") {
        85
    } else if model.contains("sonnet-3.5") || model.contains("gpt-4-turbo") {
        80
    } else if model.contains("sonnet") || model.contains("gpt-4") {
        75
    } else if model.contains("gemini-pro") {
        70
    } else if model.contains("haiku") {
        60
    } else if model.contains("gpt-3.5") {
        50
    } else if model.contains("gemini-1.0") {
        40
    } else {
        0
    }
}
