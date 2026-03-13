// 应用配置管理

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 通用设置
    #[serde(default)]
    pub general: GeneralConfig,

    /// 日志设置
    #[serde(default)]
    pub log: LogConfig,

    /// 数据库设置
    #[serde(default)]
    pub database: DatabaseConfig,

    /// 代理设置
    #[serde(default)]
    pub proxy: ProxyConfig,

    /// 健康检查设置
    #[serde(default)]
    pub health_check: HealthCheckConfig,

    /// 熔断器设置
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,

    /// 模型降级设置
    #[serde(default)]
    pub model_fallback: ModelFallbackConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// 默认 CLI 工具
    #[serde(default = "default_app")]
    pub default_app: String,

    /// 自动同步配置到 CLI 工具
    #[serde(default = "default_true")]
    pub auto_sync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// 日志级别: trace, debug, info, warn, error
    #[serde(default = "default_log_level")]
    pub level: String,

    /// 日志文件路径
    #[serde(default = "default_log_file")]
    pub file: Option<String>,

    /// 是否输出到 stderr
    #[serde(default)]
    pub stderr: bool,

    /// 日志文件最大大小 (MB)
    #[serde(default = "default_log_max_size")]
    pub max_size_mb: u64,

    /// 保留的日志文件数量
    #[serde(default = "default_log_max_backups")]
    pub max_backups: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// 数据库文件路径
    #[serde(default = "default_db_path")]
    pub path: String,

    /// WAL 模式
    #[serde(default = "default_true")]
    pub wal_mode: bool,

    /// 最大请求日志数量（超过此数量将触发清理）
    #[serde(default = "default_max_request_logs")]
    pub max_request_logs: i64,

    /// 归档目录路径
    #[serde(default = "default_archive_dir")]
    pub archive_dir: String,

    /// 是否启用自动清理
    #[serde(default = "default_true")]
    pub auto_cleanup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// 代理端口
    #[serde(default = "default_proxy_port")]
    pub port: u16,

    /// 绑定地址
    #[serde(default = "default_bind_addr")]
    pub bind: String,

    /// 请求超时 (秒)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// 流式响应超时配置
    #[serde(default)]
    pub streaming_timeout: StreamingTimeoutConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingTimeoutConfig {
    /// 首字节超时 (秒) - 等待第一个数据块的最长时间
    #[serde(default = "default_first_byte_timeout")]
    pub first_byte_secs: u64,

    /// 空闲超时 (秒) - 数据块之间的最长间隔时间
    #[serde(default = "default_idle_timeout")]
    pub idle_secs: u64,

    /// 总超时 (秒) - 整个流式响应的最长时间
    #[serde(default = "default_total_timeout")]
    pub total_secs: u64,
}

fn default_first_byte_timeout() -> u64 { 10 }
fn default_idle_timeout() -> u64 { 30 }
fn default_total_timeout() -> u64 { 300 }

impl Default for StreamingTimeoutConfig {
    fn default() -> Self {
        Self {
            first_byte_secs: 10,
            idle_secs: 30,
            total_secs: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// 检查间隔 (秒)
    #[serde(default = "default_health_interval")]
    pub interval_secs: u64,

    /// 是否启用自动健康检查
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// 失败阈值 (连续失败多少次后打开熔断器)
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,

    /// 成功阈值 (半开状态下连续成功多少次后关闭熔断器)
    #[serde(default = "default_success_threshold")]
    pub success_threshold: u32,

    /// 熔断超时时间 (秒) - Open 状态持续多久后进入 Half-Open
    #[serde(default = "default_circuit_timeout")]
    pub timeout_secs: u64,

    /// 半开状态超时时间 (秒)
    #[serde(default = "default_half_open_timeout")]
    pub half_open_timeout_secs: u64,
}

// 默认值函数
fn default_app() -> String { "claude-code".to_string() }
fn default_true() -> bool { true }
fn default_log_level() -> String { "info".to_string() }
fn default_log_file() -> Option<String> { Some("~/.mrouter/logs/mrouter.log".to_string()) }
fn default_log_max_size() -> u64 { 10 } // 10 MB
fn default_log_max_backups() -> usize { 5 } // 保留 5 个备份
fn default_db_path() -> String { "~/.mrouter/db/mrouter.db".to_string() }
fn default_proxy_port() -> u16 { 4444 }
fn default_bind_addr() -> String { "127.0.0.1".to_string() }
fn default_timeout() -> u64 { 30 }
fn default_health_interval() -> u64 { 300 }
fn default_failure_threshold() -> u32 { 5 }
fn default_success_threshold() -> u32 { 2 }
fn default_circuit_timeout() -> u64 { 60 }
fn default_half_open_timeout() -> u64 { 30 }
fn default_max_request_logs() -> i64 { 1_000_000 }
fn default_archive_dir() -> String { "~/.mrouter/archives".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFallbackConfig {
    /// 是否启用智能模型降级
    #[serde(default)]
    pub enabled: bool,

    /// 模型降级映射表 (原模型 -> 降级模型列表)
    #[serde(default = "default_fallback_chains")]
    pub fallback_chains: std::collections::HashMap<String, Vec<String>>,
}

fn default_fallback_chains() -> std::collections::HashMap<String, Vec<String>> {
    let mut chains = std::collections::HashMap::new();

    // Claude 系列降级链
    chains.insert("claude-opus-4".to_string(), vec!["claude-sonnet-4".to_string(), "claude-haiku-4".to_string()]);
    chains.insert("claude-opus-4-20250514".to_string(), vec!["claude-sonnet-4-20250514".to_string(), "claude-haiku-4-20250514".to_string()]);
    chains.insert("claude-sonnet-4".to_string(), vec!["claude-haiku-4".to_string()]);
    chains.insert("claude-sonnet-4-20250514".to_string(), vec!["claude-haiku-4-20250514".to_string()]);

    // GPT 系列降级链
    chains.insert("gpt-4".to_string(), vec!["gpt-4-turbo".to_string(), "gpt-3.5-turbo".to_string()]);
    chains.insert("gpt-4-turbo".to_string(), vec!["gpt-3.5-turbo".to_string()]);
    chains.insert("gpt-4o".to_string(), vec!["gpt-4-turbo".to_string(), "gpt-3.5-turbo".to_string()]);

    // Gemini 系列降级链
    chains.insert("gemini-pro".to_string(), vec!["gemini-pro-vision".to_string()]);
    chains.insert("gemini-1.5-pro".to_string(), vec!["gemini-1.0-pro".to_string()]);

    chains
}

impl Default for ModelFallbackConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            fallback_chains: default_fallback_chains(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            log: LogConfig::default(),
            database: DatabaseConfig::default(),
            proxy: ProxyConfig::default(),
            health_check: HealthCheckConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
            model_fallback: ModelFallbackConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            default_app: default_app(),
            auto_sync: true,
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: default_log_file(),
            stderr: false,
            max_size_mb: default_log_max_size(),
            max_backups: default_log_max_backups(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_db_path(),
            wal_mode: true,
            max_request_logs: default_max_request_logs(),
            archive_dir: default_archive_dir(),
            auto_cleanup: true,
        }
    }
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            port: default_proxy_port(),
            bind: default_bind_addr(),
            timeout_secs: default_timeout(),
            streaming_timeout: StreamingTimeoutConfig::default(),
        }
    }
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval_secs: default_health_interval(),
            enabled: false,
        }
    }
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: default_failure_threshold(),
            success_threshold: default_success_threshold(),
            timeout_secs: default_circuit_timeout(),
            half_open_timeout_secs: default_half_open_timeout(),
        }
    }
}

impl AppConfig {
    /// 获取配置文件路径
    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
        Ok(home.join(".mrouter").join("config.toml"))
    }

    /// 加载配置 (不存在则创建默认)
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let config: AppConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = AppConfig::default();
            config.save()?;
            Ok(config)
        }
    }

    /// 保存配置到文件
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;

        Ok(())
    }

    /// 解析数据库路径 (展开 ~)
    pub fn resolve_db_path(&self) -> Result<PathBuf> {
        let path_str = &self.database.path;
        if path_str.starts_with("~/") {
            let home = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
            Ok(home.join(&path_str[2..]))
        } else {
            Ok(PathBuf::from(path_str))
        }
    }

    /// 解析日志文件路径
    pub fn resolve_log_path(&self) -> Option<PathBuf> {
        self.log.file.as_ref().map(|path_str| {
            if path_str.starts_with("~/") {
                dirs::home_dir()
                    .map(|home| home.join(&path_str[2..]))
                    .unwrap_or_else(|| PathBuf::from(path_str))
            } else {
                PathBuf::from(path_str)
            }
        })
    }
}
