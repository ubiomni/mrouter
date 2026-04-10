// 模型列表缓存服务

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::fs;

use crate::models::ProviderType;

/// 模型列表缓存
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCache {
    /// 缓存的模型列表（key: provider_type, value: models）
    pub models: HashMap<String, Vec<String>>,
    /// 缓存时间
    pub cached_at: SystemTime,
    /// 缓存有效期（秒）
    pub ttl: u64,
}

impl ModelCache {
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            cached_at: SystemTime::now(),
            ttl: 86400, // 24 小时
        }
    }

    /// 检查缓存是否过期
    pub fn is_expired(&self) -> bool {
        if let Ok(elapsed) = self.cached_at.elapsed() {
            elapsed.as_secs() > self.ttl
        } else {
            true
        }
    }

    /// 获取缓存路径
    fn cache_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
        let cache_dir = home.join(".mrouter").join("cache");
        std::fs::create_dir_all(&cache_dir)?;
        Ok(cache_dir.join("models.json"))
    }

    /// 从文件加载缓存
    pub async fn load() -> Result<Self> {
        let path = Self::cache_path()?;
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(&path).await?;
        let cache: ModelCache = serde_json::from_str(&content)?;
        Ok(cache)
    }

    /// 保存缓存到文件
    pub async fn save(&self) -> Result<()> {
        let path = Self::cache_path()?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content).await?;
        Ok(())
    }

    /// 获取指定 Provider 的模型列表
    pub fn get_models(&self, provider_type: &ProviderType) -> Option<Vec<String>> {
        self.models.get(provider_type.as_str()).cloned()
    }

    /// 设置指定 Provider 的模型列表
    pub fn set_models(&mut self, provider_type: &ProviderType, models: Vec<String>) {
        self.models.insert(provider_type.as_str().to_string(), models);
        self.cached_at = SystemTime::now();
    }
}

/// 模型列表服务
pub struct ModelService;

impl ModelService {
    /// 获取 OpenAI 兼容格式的模型列表（通用方法）
    async fn fetch_openai_compatible_models(base_url: &str, api_key: &str, filter_prefix: Option<&str>) -> Result<Vec<String>> {
        let client = reqwest::Client::new();
        let url = format!("{}/v1/models", base_url.trim_end_matches('/'));

        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .timeout(Duration::from_secs(10))
            .send()
            .await?;

        #[derive(Deserialize)]
        struct ModelList {
            data: Vec<Model>,
        }

        #[derive(Deserialize)]
        struct Model {
            id: String,
        }

        let model_list: ModelList = response.json().await?;
        let mut models: Vec<String> = model_list
            .data
            .into_iter()
            .map(|m| m.id)
            .collect();

        // 如果指定了前缀过滤，只保留匹配的模型
        if let Some(prefix) = filter_prefix {
            models.retain(|m| m.starts_with(prefix));
        }

        Ok(models)
    }

    /// 获取 OpenAI 的模型列表
    async fn fetch_openai_models(api_key: &str) -> Result<Vec<String>> {
        Self::fetch_openai_compatible_models("https://api.openai.com", api_key, Some("gpt-")).await
    }

    /// 获取 DeepSeek 的模型列表
    async fn fetch_deepseek_models(api_key: &str) -> Result<Vec<String>> {
        Self::fetch_openai_compatible_models("https://api.deepseek.com", api_key, Some("deepseek-")).await
    }

    /// 获取 xAI 的模型列表
    async fn fetch_xai_models(api_key: &str) -> Result<Vec<String>> {
        Self::fetch_openai_compatible_models("https://api.x.ai", api_key, Some("grok-")).await
    }

    /// 获取 Moonshot 的模型列表
    async fn fetch_moonshot_models(api_key: &str) -> Result<Vec<String>> {
        Self::fetch_openai_compatible_models("https://api.moonshot.cn", api_key, Some("moonshot-")).await
    }

    /// 获取 MiniMax 的模型列表
    async fn fetch_minimax_models(api_key: &str) -> Result<Vec<String>> {
        Self::fetch_openai_compatible_models("https://api.minimax.chat", api_key, None).await
    }

    /// 获取 Zhipu 的模型列表
    async fn fetch_zhipu_models(api_key: &str) -> Result<Vec<String>> {
        Self::fetch_openai_compatible_models("https://open.bigmodel.cn/api/paas", api_key, Some("glm-")).await
    }

    /// 获取 Anthropic 的模型列表（目前 Anthropic 没有公开的 models API，使用默认列表）
    fn fetch_anthropic_models() -> Vec<String> {
        vec![
            "claude-opus-4".to_string(),
            "claude-sonnet-4".to_string(),
            "claude-haiku-4".to_string(),
            "claude-opus-4-20250514".to_string(),
            "claude-sonnet-4-20250514".to_string(),
            "claude-haiku-4-20250514".to_string(),
        ]
    }

    /// 获取 Google 的模型列表
    async fn fetch_google_models(api_key: &str) -> Result<Vec<String>> {
        let client = reqwest::Client::new();
        let response = client
            .get("https://generativelanguage.googleapis.com/v1beta/models")
            .query(&[("key", api_key)])
            .timeout(Duration::from_secs(10))
            .send()
            .await?;

        #[derive(Deserialize)]
        struct ModelList {
            models: Vec<Model>,
        }

        #[derive(Deserialize)]
        struct Model {
            name: String,
        }

        let model_list: ModelList = response.json().await?;
        let models: Vec<String> = model_list
            .models
            .into_iter()
            .filter_map(|m| {
                // 提取模型 ID（去掉 "models/" 前缀）
                m.name.strip_prefix("models/").map(|s| s.to_string())
            })
            .filter(|m| {
                // 只保留 gemini 模型
                m.starts_with("gemini")
            })
            .collect();

        Ok(models)
    }

    /// 从 API 获取模型列表（带重试和超时）
    ///
    /// # 参数
    /// - `provider_type`: Provider 类型
    /// - `api_key`: API 密钥
    /// - `base_url`: 可选的自定义 base_url（用于 Custom 类型）
    pub async fn fetch_models(
        provider_type: &ProviderType,
        api_key: Option<&str>,
        base_url: Option<&str>,
    ) -> Result<Vec<String>> {
        match provider_type {
            ProviderType::OpenAI => {
                if let Some(key) = api_key {
                    Self::fetch_openai_models(key).await
                } else {
                    Err(anyhow::anyhow!("API key required for OpenAI"))
                }
            }
            ProviderType::Anthropic => {
                // Anthropic 没有公开的 models API，使用默认列表
                Ok(Self::fetch_anthropic_models())
            }
            ProviderType::Google => {
                if let Some(key) = api_key {
                    Self::fetch_google_models(key).await
                } else {
                    Err(anyhow::anyhow!("API key required for Google"))
                }
            }
            ProviderType::DeepSeek => {
                if let Some(key) = api_key {
                    Self::fetch_deepseek_models(key).await
                } else {
                    Err(anyhow::anyhow!("API key required for DeepSeek"))
                }
            }
            ProviderType::XAI => {
                if let Some(key) = api_key {
                    Self::fetch_xai_models(key).await
                } else {
                    Err(anyhow::anyhow!("API key required for xAI"))
                }
            }
            ProviderType::Moonshot => {
                if let Some(key) = api_key {
                    Self::fetch_moonshot_models(key).await
                } else {
                    Err(anyhow::anyhow!("API key required for Moonshot"))
                }
            }
            ProviderType::MiniMax => {
                if let Some(key) = api_key {
                    Self::fetch_minimax_models(key).await
                } else {
                    Err(anyhow::anyhow!("API key required for MiniMax"))
                }
            }
            ProviderType::Zhipu => {
                if let Some(key) = api_key {
                    Self::fetch_zhipu_models(key).await
                } else {
                    Err(anyhow::anyhow!("API key required for Zhipu"))
                }
            }
            // 对于其他 Provider，尝试使用 OpenAI 兼容格式获取
            // 如果失败，降级到默认列表
            _ => {
                if let Some(key) = api_key {
                    // 优先使用用户提供的 base_url（用于 Custom 类型）
                    let url = if let Some(custom_url) = base_url {
                        custom_url
                    } else {
                        provider_type.default_base_url()
                    };

                    if !url.is_empty() {
                        match Self::fetch_openai_compatible_models(url, key, None).await {
                            Ok(models) if !models.is_empty() => {
                                tracing::info!("Successfully fetched models from {} using OpenAI-compatible API", provider_type.display_name());
                                return Ok(models);
                            }
                            Err(e) => {
                                tracing::debug!("Failed to fetch models from {} using OpenAI-compatible API: {}", provider_type.display_name(), e);
                            }
                            _ => {}
                        }
                    }
                }
                // 降级到默认列表
                Ok(provider_type.default_supported_models())
            }
        }
    }

    /// 更新缓存（后台任务）
    pub async fn update_cache_background(
        provider_type: ProviderType,
        api_key: Option<String>,
        base_url: Option<String>,
    ) {
        tokio::spawn(async move {
            match Self::update_cache(&provider_type, api_key.as_deref(), base_url.as_deref()).await {
                Ok(_) => {
                    tracing::info!("Model cache updated for {:?}", provider_type);
                }
                Err(e) => {
                    tracing::warn!("Failed to update model cache for {:?}: {}", provider_type, e);
                }
            }
        });
    }

    /// 更新缓存
    pub async fn update_cache(
        provider_type: &ProviderType,
        api_key: Option<&str>,
        base_url: Option<&str>,
    ) -> Result<()> {
        let mut cache = ModelCache::load().await.unwrap_or_else(|_| ModelCache::new());

        // 从 API 获取模型列表
        let models = Self::fetch_models(provider_type, api_key, base_url).await?;

        // 更新缓存
        cache.set_models(provider_type, models);
        cache.save().await?;

        Ok(())
    }

    /// 获取模型列表（优先使用缓存，缓存过期或不存在时使用默认列表）
    pub async fn get_models(provider_type: &ProviderType) -> Vec<String> {
        // 尝试从缓存加载
        if let Ok(cache) = ModelCache::load().await {
            if !cache.is_expired() {
                if let Some(models) = cache.get_models(provider_type) {
                    tracing::debug!("Using cached models for {:?}", provider_type);
                    return models;
                }
            }
        }

        // 缓存不存在或过期，使用默认列表
        tracing::debug!("Using default models for {:?}", provider_type);
        provider_type.default_supported_models()
    }
}
