// 模型映射模块
//
// 在请求转发前，根据 Provider 配置替换请求中的模型名称

use crate::models::{Provider, ProviderType};
use serde_json::Value;

/// 模型映射配置
pub struct ModelMapping {
    /// 配置的默认模型（如果 provider.model 不为空）
    pub default_model: Option<String>,
    /// Provider 类型
    #[allow(dead_code)]
    pub provider_type: ProviderType,
}

impl ModelMapping {
    /// 从 Provider 配置中提取模型映射
    pub fn from_provider(provider: &Provider) -> Self {
        Self {
            default_model: provider.model.clone(),
            provider_type: provider.provider_type.clone(),
        }
    }

    /// 检查是否配置了模型映射
    #[allow(dead_code)]
    pub fn has_mapping(&self) -> bool {
        self.default_model.is_some()
    }

    /// 根据原始模型名称获取映射后的模型
    ///
    /// 映射逻辑：
    /// 1. 如果 provider 配置了 model，则使用配置的模型（强制映射）
    /// 2. 否则，保持原样（不做任何映射）
    pub fn map_model(&self, original_model: &str) -> String {
        // 如果配置了默认模型，直接使用
        if let Some(ref m) = self.default_model {
            return m.clone();
        }

        // 没有配置映射，保持原样
        original_model.to_string()
    }

}

/// 对请求体应用模型映射
///
/// 返回 (映射后的请求体, 原始模型名, 映射后模型名)
pub fn apply_model_mapping(
    mut body: Value,
    provider: &Provider,
) -> (Value, Option<String>, Option<String>) {
    let mapping = ModelMapping::from_provider(provider);

    // 提取原始模型名
    let original_model = body.get("model").and_then(|m| m.as_str()).map(String::from);

    if let Some(ref original) = original_model {
        let mapped = mapping.map_model(original);

        if mapped != *original {
            tracing::info!("[ModelMapper] Model mapping: {} → {} (Provider: {})",
                original, mapped, provider.name);
            body["model"] = serde_json::json!(mapped);
            return (body, Some(original.clone()), Some(mapped));
        }
    }

    (body, original_model, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Provider, ProviderType, AppType};
    use serde_json::json;

    fn create_anthropic_provider() -> Provider {
        Provider::new(
            AppType::ClaudeCode,
            ProviderType::Anthropic,
            "Test Anthropic".to_string(),
            "sk-test".to_string(),
            "https://api.anthropic.com".to_string(),
        )
    }

    #[test]
    fn test_provider_with_configured_model() {
        let mut provider = create_anthropic_provider();
        provider.model = Some("custom-model".to_string());
        let body = json!({"model": "claude-sonnet-4-5-20250929"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "custom-model");
        assert_eq!(mapped, Some("custom-model".to_string()));
    }

    #[test]
    fn test_no_mapping_without_config() {
        let provider = create_anthropic_provider();
        let body = json!({"model": "claude-sonnet-4-5-20250929"});
        let (result, original, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "claude-sonnet-4-5-20250929");
        assert_eq!(original, Some("claude-sonnet-4-5-20250929".to_string()));
        assert!(mapped.is_none());
    }
}
