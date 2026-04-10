// 配置文件同步服务

use anyhow::Result;
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use std::fs;
use chrono::Utc;
use crate::models::{Provider, ProviderType, AppType, ApiFormat};

/// 配置文件格式
#[derive(Debug, Clone, Copy)]
pub enum ConfigFormat {
    Json,
    Toml,
    Env,
}

/// 配置同步服务
pub struct ConfigSyncService;

impl ConfigSyncService {
    /// 获取 CLI 工具的配置文件路径
    pub fn get_config_path(app_type: AppType) -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;

        let path = match app_type {
            AppType::ClaudeCode => home.join(".claude").join("settings.json"),
            AppType::Codex => home.join(".codex").join("config.json"),
            AppType::GeminiCli => home.join(".config").join("gemini").join("config.json"),
            AppType::OpenCode => home.join(".opencode").join("config.json"),
            AppType::OpenClaw => home.join(".openclaw").join(".env"),
        };

        Ok(path)
    }
    
    /// 同步 Provider 配置到 CLI 工具配置文件
    pub fn sync_to_file(provider: &Provider) -> Result<()> {
        let config_path = Self::get_config_path(provider.app_type)?;

        // 如果配置文件已存在，先备份
        if config_path.exists() {
            Self::backup_config(provider.app_type)?;
        }

        // 确保目录存在
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        match provider.app_type {
            AppType::OpenClaw => {
                // OpenClaw 使用 .env 格式 + openclaw.json
                Self::write_env_file(&config_path, provider)?;
                Self::write_openclaw_json(provider)?;
            }
            _ => {
                // 其他使用 JSON 格式
                Self::write_json_file(&config_path, provider)?;
            }
        }

        tracing::info!("Synced provider '{}' to {:?}", provider.name, config_path);
        Ok(())
    }
    
    /// 写入 JSON 配置文件
    fn write_json_file(path: &PathBuf, provider: &Provider) -> Result<()> {
        let mut config = if path.exists() {
            let content = fs::read_to_string(path)?;
            serde_json::from_str::<JsonValue>(&content).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        // Claude Code 使用特殊的 env 格式
        if provider.app_type == AppType::ClaudeCode {
            if let Some(obj) = config.as_object_mut() {
                // 确保 env 对象存在
                if !obj.contains_key("env") {
                    obj.insert("env".to_string(), serde_json::json!({}));
                }

                if let Some(env_obj) = obj.get_mut("env").and_then(|v| v.as_object_mut()) {
                    // 更新环境变量
                    env_obj.insert("ANTHROPIC_API_KEY".to_string(), JsonValue::String(provider.api_key.clone()));
                    env_obj.insert("ANTHROPIC_AUTH_TOKEN".to_string(), JsonValue::String(provider.api_key.clone()));
                    env_obj.insert("ANTHROPIC_BASE_URL".to_string(), JsonValue::String(provider.base_url.clone()));

                    if let Some(model) = &provider.model {
                        env_obj.insert("ANTHROPIC_MODEL".to_string(), JsonValue::String(model.clone()));
                    }
                }
            }
        } else {
            // 其他 CLI Tools 使用标准格式
            if let Some(obj) = config.as_object_mut() {
                obj.insert("api_key".to_string(), JsonValue::String(provider.api_key.clone()));
                obj.insert("base_url".to_string(), JsonValue::String(provider.base_url.clone()));

                if let Some(model) = &provider.model {
                    obj.insert("model".to_string(), JsonValue::String(model.clone()));
                }

                // 合并自定义配置
                if let Some(custom_config) = provider.config.as_object() {
                    for (key, value) in custom_config {
                        obj.insert(key.clone(), value.clone());
                    }
                }
            }
        }

        // 写入文件
        let content = serde_json::to_string_pretty(&config)?;
        fs::write(path, content)?;

        Ok(())
    }
    
    /// 将 Provider 名称转换为 env 变量后缀（大写，非字母数字替换为下划线）
    fn provider_env_suffix(name: &str) -> String {
        name.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
            .collect()
    }

    /// 根据 Provider 的 effective_api_format 返回 OpenClaw 的 api 格式字符串
    /// mrouter 是透传代理，不做 API 格式转换，所以 OpenClaw 需要使用上游实际支持的 API 格式
    fn openclaw_api_format(provider: &Provider) -> &'static str {
        match provider.effective_api_format() {
            ApiFormat::Anthropic => "anthropic-messages",
            ApiFormat::Google => "google-generative-ai",
            ApiFormat::OpenAI => "openai-completions",
        }
    }

    /// 写入 .env 配置文件（按 Provider 名称使用独立变量）
    fn write_env_file(path: &PathBuf, provider: &Provider) -> Result<()> {
        let suffix = Self::provider_env_suffix(&provider.name);
        let key_prefix_api = format!("API_KEY_{}", suffix);
        let key_prefix_base = format!("BASE_URL_{}", suffix);
        let key_prefix_model = format!("MODEL_{}", suffix);

        let mut lines = Vec::new();

        // 读取现有内容，保留其他 Provider 的变量
        if path.exists() {
            let content = fs::read_to_string(path)?;
            for line in content.lines() {
                // 跳过当前 Provider 的旧变量（会重新写入）
                if line.starts_with(&format!("{}=", key_prefix_api))
                    || line.starts_with(&format!("{}=", key_prefix_base))
                    || line.starts_with(&format!("{}=", key_prefix_model))
                {
                    continue;
                }
                lines.push(line.to_string());
            }
        }

        // 添加当前 Provider 的变量
        lines.push(format!("{}={}", key_prefix_api, provider.api_key));
        lines.push(format!("{}={}", key_prefix_base, provider.base_url));

        if let Some(model) = &provider.model {
            lines.push(format!("{}={}", key_prefix_model, model));
        }

        // 写入文件
        fs::write(path, lines.join("\n"))?;

        Ok(())
    }
    
    /// 写入 openclaw.json 配置文件（按 Provider 名称创建独立条目）
    fn write_openclaw_json(provider: &Provider) -> Result<()> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
        let json_path = home.join(".openclaw").join("openclaw.json");

        // 确保目录存在
        if let Some(parent) = json_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // 备份现有 openclaw.json
        if json_path.exists() {
            let timestamp = Utc::now().format("%Y%m%d_%H%M%S_%3f").to_string();
            let backup_path = json_path.with_file_name(format!("openclaw.backup_{}.json", timestamp));
            fs::copy(&json_path, &backup_path)?;
            tracing::info!("Backed up openclaw.json to {:?}", backup_path);
        }

        // 读取现有 JSON 或创建空对象
        let mut config = if json_path.exists() {
            let content = fs::read_to_string(&json_path)?;
            serde_json::from_str::<JsonValue>(&content).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        let suffix = Self::provider_env_suffix(&provider.name);
        // Provider name sanitized for JSON key: lowercase, non-alphanumeric → hyphen
        let provider_key = format!("custom-{}", provider.name.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
            .collect::<String>());

        // 根据 provider_type 选择 OpenClaw 的 api 格式
        // mrouter 是透传代理，OpenClaw 发什么格式的请求，上游就收到什么格式
        // 所以 api 格式必须匹配上游 provider 实际支持的 API
        let api_format = Self::openclaw_api_format(provider);

        // 构建 provider 条目，引用 .env 中对应的变量
        let provider_entry = serde_json::json!({
            "baseUrl": format!("${{BASE_URL_{}}}", suffix),
            "apiKey": format!("${{API_KEY_{}}}", suffix),
            "api": api_format,
            "models": [{
                "id": format!("${{MODEL_{}}}", suffix),
                "name": provider.name
            }]
        });

        // 确保 models.providers 存在
        let obj = config.as_object_mut().ok_or_else(|| anyhow::anyhow!("openclaw.json root is not an object"))?;
        if !obj.contains_key("models") {
            obj.insert("models".to_string(), serde_json::json!({}));
        }
        let models = obj.get_mut("models").unwrap().as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("models is not an object"))?;
        if !models.contains_key("providers") {
            models.insert("providers".to_string(), serde_json::json!({}));
        }
        let providers = models.get_mut("providers").unwrap().as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("models.providers is not an object"))?;

        // 插入/更新当前 Provider 条目，保留其他 providers 不变
        providers.insert(provider_key.clone(), provider_entry);

        // 更新 agents.defaults.model.primary 指向最后同步的 Provider
        if !obj.contains_key("agents") {
            obj.insert("agents".to_string(), serde_json::json!({}));
        }
        let agents = obj.get_mut("agents").unwrap().as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("agents is not an object"))?;
        if !agents.contains_key("defaults") {
            agents.insert("defaults".to_string(), serde_json::json!({}));
        }
        let defaults = agents.get_mut("defaults").unwrap().as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("agents.defaults is not an object"))?;
        if !defaults.contains_key("model") {
            defaults.insert("model".to_string(), serde_json::json!({}));
        }
        let model_obj = defaults.get_mut("model").unwrap().as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("agents.defaults.model is not an object"))?;
        model_obj.insert(
            "primary".to_string(),
            JsonValue::String(format!("{}/{}",
                provider_key,
                format!("${{MODEL_{}}}", suffix),
            )),
        );

        // 写入文件
        let content = serde_json::to_string_pretty(&config)?;
        fs::write(&json_path, content)?;

        tracing::info!("Updated openclaw.json with provider '{}'", provider_key);
        Ok(())
    }

    /// 从配置文件读取并创建 Provider
    pub fn load_from_file(app_type: AppType) -> Result<Option<Provider>> {
        let config_path = Self::get_config_path(app_type)?;
        
        if !config_path.exists() {
            return Ok(None);
        }
        
        match app_type {
            AppType::OpenClaw => Self::load_from_env_file(&config_path, app_type),
            _ => Self::load_from_json_file(&config_path, app_type),
        }
    }
    
    /// 从 JSON 文件加载
    fn load_from_json_file(path: &PathBuf, app_type: AppType) -> Result<Option<Provider>> {
        let content = fs::read_to_string(path)?;
        let config: JsonValue = serde_json::from_str(&content)?;
        
        if let Some(obj) = config.as_object() {
            let api_key = obj.get("api_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            
            let base_url = obj.get("base_url")
                .and_then(|v| v.as_str())
                .unwrap_or("https://api.anthropic.com")
                .to_string();
            
            let model = obj.get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            
            if !api_key.is_empty() {
                let mut provider = Provider::new(
                    app_type,
                    ProviderType::Custom,
                    "Imported".to_string(),
                    api_key,
                    base_url,
                );
                provider.model = model;
                provider.config = config;

                return Ok(Some(provider));
            }
        }

        Ok(None)
    }

    /// 从 .env 文件加载
    fn load_from_env_file(path: &PathBuf, app_type: AppType) -> Result<Option<Provider>> {
        let content = fs::read_to_string(path)?;
        let mut api_key = String::new();
        let mut base_url = String::new();
        let mut model = None;

        for line in content.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key.trim() {
                    "API_KEY" => api_key = value.trim().to_string(),
                    "BASE_URL" => base_url = value.trim().to_string(),
                    "MODEL" => model = Some(value.trim().to_string()),
                    _ => {}
                }
            }
        }

        if !api_key.is_empty() {
            let mut provider = Provider::new(
                app_type,
                ProviderType::Custom,
                "Imported".to_string(),
                api_key,
                base_url,
            );
            provider.model = model;
            
            return Ok(Some(provider));
        }
        
        Ok(None)
    }
    
    /// 备份配置文件（使用毫秒级时间戳）
    pub fn backup_config(app_type: AppType) -> Result<PathBuf> {
        let config_path = Self::get_config_path(app_type)?;

        if !config_path.exists() {
            return Err(anyhow::anyhow!("Config file does not exist"));
        }

        // 生成毫秒级时间戳
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S_%3f").to_string();

        // 获取原文件名和扩展名
        let file_stem = config_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("config");
        let extension = config_path.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        // 构建备份文件名：原文件名.backup_时间戳.扩展名
        let backup_filename = if extension.is_empty() {
            format!("{}.backup_{}", file_stem, timestamp)
        } else {
            format!("{}.backup_{}.{}", file_stem, timestamp, extension)
        };

        let backup_path = config_path.with_file_name(backup_filename);
        fs::copy(&config_path, &backup_path)?;

        tracing::info!("Backed up config to {:?}", backup_path);
        Ok(backup_path)
    }
    
    /// 恢复配置文件
    pub fn restore_config(app_type: AppType) -> Result<()> {
        let config_path = Self::get_config_path(app_type)?;
        let backup_path = config_path.with_extension("backup");
        
        if !backup_path.exists() {
            return Err(anyhow::anyhow!("Backup file does not exist"));
        }
        
        fs::copy(&backup_path, &config_path)?;
        
        tracing::info!("Restored config from {:?}", backup_path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_paths() {
        for app_type in AppType::all() {
            let path = ConfigSyncService::get_config_path(app_type);
            assert!(path.is_ok());
        }
    }

    #[test]
    fn test_backup_filename_format() {
        // 测试毫秒级时间戳格式
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S_%3f").to_string();
        println!("Generated timestamp: {}", timestamp);

        // 验证格式：YYYYMMDD_HHMMSS_mmm
        assert!(timestamp.len() >= 18); // 至少 20260307_203944_123
        assert!(timestamp.contains('_'));

        // 测试备份文件名生成
        let backup_filename = format!("settings.backup_{}.json", timestamp);
        println!("Backup filename: {}", backup_filename);
        assert!(backup_filename.starts_with("settings.backup_"));
        assert!(backup_filename.ends_with(".json"));
    }
}
