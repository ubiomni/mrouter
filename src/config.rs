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

    /// 上游 HTTP/SOCKS5 代理（如 socks5://127.0.0.1:7890, http://proxy:8080）
    /// 设为 "none" 禁用系统代理；不设置则跟随系统环境变量
    #[serde(default)]
    pub upstream_proxy: Option<String>,

    /// 全局自定义请求头（转发到上游时附加，Provider 级别的 custom_headers 优先级更高）
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,

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
            upstream_proxy: None,
            headers: std::collections::HashMap::new(),
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
            config.save_default_template()?;
            Ok(config)
        }
    }

    /// 保存配置到文件（保留已有注释，更新变化的值并移除其行内注释）
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let new_toml = toml::to_string_pretty(self)?;

        if path.exists() {
            let existing_content = fs::read_to_string(&path)?;
            match (
                existing_content.parse::<toml_edit::DocumentMut>(),
                new_toml.parse::<toml_edit::DocumentMut>(),
            ) {
                (Ok(mut doc), Ok(new_doc)) => {
                    Self::merge_tables(doc.as_table_mut(), new_doc.as_table());
                    fs::write(&path, doc.to_string())?;
                }
                _ => {
                    // 解析失败则直接覆盖
                    fs::write(&path, new_toml)?;
                }
            }
        } else {
            fs::write(&path, new_toml)?;
        }

        Ok(())
    }

    /// 递归合并新配置到已有文档，保留未修改项的注释
    fn merge_tables(existing: &mut toml_edit::Table, new_values: &toml_edit::Table) {
        for (key, new_item) in new_values.iter() {
            match new_item {
                toml_edit::Item::Table(new_sub) => {
                    if let Some(toml_edit::Item::Table(existing_sub)) = existing.get_mut(key) {
                        Self::merge_tables(existing_sub, new_sub);
                    } else {
                        existing.insert(key, new_item.clone());
                    }
                }
                toml_edit::Item::Value(new_val) => {
                    let changed = existing
                        .get(key)
                        .and_then(|item| item.as_value())
                        .map(|old_val| !Self::values_equal(old_val, new_val))
                        .unwrap_or(true);

                    if changed {
                        // 值变了：更新值，行内注释自然被移除（new_val 无注释）
                        existing.insert(key, toml_edit::Item::Value(new_val.clone()));
                    }
                    // 值未变：保留原样（包括行内注释）
                }
                _ => {
                    existing.insert(key, new_item.clone());
                }
            }
        }
    }

    /// 比较两个 toml_edit::Value 的实际内容（忽略注释/空白装饰）
    fn values_equal(a: &toml_edit::Value, b: &toml_edit::Value) -> bool {
        let mut a = a.clone();
        let mut b = b.clone();
        a.decor_mut().set_prefix("");
        a.decor_mut().set_suffix("");
        b.decor_mut().set_prefix("");
        b.decor_mut().set_suffix("");
        a.to_string().trim() == b.to_string().trim()
    }

    /// 首次创建时写入带注释的完整配置模板
    fn save_default_template(&self) -> Result<()> {
        let path = Self::config_path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let template = r#"# ============================================================
#  MRouter Configuration
#  https://github.com/ubiomni/mrouter
# ============================================================

# ---- General Settings ----
[general]
default_app = "claude-code"          # Default CLI tool: claude-code, codex, gemini-cli, opencode, openclaw
auto_sync = true                     # Auto-sync config to CLI tools when switching providers

# ---- Logging ----
[log]
level = "info"                       # Log level: trace, debug, info, warn, error
file = "~/.mrouter/logs/mrouter.log" # Log file path (remove to disable file logging)
stderr = false                       # Also output to stderr
max_size_mb = 10                     # Max log file size (MB), auto-rotates when exceeded
max_backups = 5                      # Number of rotated log files to keep

# ---- Database ----
[database]
path = "~/.mrouter/db/mrouter.db"   # SQLite database path
wal_mode = true                      # Enable WAL mode (recommended)
auto_cleanup = true                  # Auto-cleanup old request logs on proxy start
max_request_logs = 1000000           # Max request log entries before cleanup
archive_dir = "~/.mrouter/archives"  # Archive directory for cleaned-up logs

# ---- Proxy Server ----
[proxy]
port = 4444                          # Proxy listen port
bind = "127.0.0.1"                   # Bind address ("0.0.0.0" to allow external access)
timeout_secs = 30                    # HTTP connection timeout (seconds)

# Upstream proxy for outbound requests to AI providers
# - Set a URL to force all requests through that proxy
# - Set "none" to disable system proxy (ignore http_proxy/https_proxy env vars)
# - Leave commented to follow system environment variables (default)
#
# upstream_proxy = "socks5://127.0.0.1:7890"
# upstream_proxy = "http://proxy.corp:8080"
# upstream_proxy = "none"

# Global custom headers (applied to ALL upstream provider requests)
#
# Header priority (highest wins):
#   1. Provider custom_headers (TUI 'o' key, per-provider)
#   2. [proxy.headers] below (global, this file)
#   3. Client original headers (passthrough)
#
# [proxy.headers]
# User-Agent = "claude-cli/2.1.72 (external, cli)"
# X-Custom-Header = "some-value"

# Streaming response timeouts
[proxy.streaming_timeout]
first_byte_secs = 10                 # Max wait for first data chunk
idle_secs = 30                       # Max gap between data chunks
total_secs = 300                     # Max total streaming duration (5 min)

# ---- Health Check ----
[health_check]
enabled = false                      # Enable periodic health checks
interval_secs = 300                  # Check interval (seconds), default 5 min

# ---- Circuit Breaker ----
[circuit_breaker]
failure_threshold = 5                # Consecutive failures to trip the breaker (Open)
success_threshold = 2                # Consecutive successes in Half-Open to recover (Closed)
timeout_secs = 60                    # How long Open state lasts before Half-Open (seconds)
half_open_timeout_secs = 30          # Half-Open state timeout (seconds)

# ---- Model Fallback ----
#
# When enabled, if the requested model fails on all providers,
# mrouter will retry with fallback models in order.
#
[model_fallback]
enabled = false                      # Enable smart model fallback/degradation

# Fallback chains: requested model -> list of fallback models to try
[model_fallback.fallback_chains]
"claude-opus-4" = ["claude-sonnet-4", "claude-haiku-4"]
"claude-opus-4-20250514" = ["claude-sonnet-4-20250514", "claude-haiku-4-20250514"]
"claude-sonnet-4" = ["claude-haiku-4"]
"claude-sonnet-4-20250514" = ["claude-haiku-4-20250514"]
"gpt-4" = ["gpt-4-turbo", "gpt-3.5-turbo"]
"gpt-4-turbo" = ["gpt-3.5-turbo"]
"gpt-4o" = ["gpt-4-turbo", "gpt-3.5-turbo"]
"gemini-pro" = ["gemini-pro-vision"]
"gemini-1.5-pro" = ["gemini-1.0-pro"]

# ============================================================
#  Provider-Level Settings (configured via TUI, not this file)
# ============================================================
#
# Select a provider in TUI and press the shortcut key:
#
#   'o' - Auth Header & Custom Headers (JSON)
#         Per-provider headers take highest priority, override [proxy.headers]
#   'e' - Edit provider (name, base_url, api_key, etc.)
#   'm' - Model Mappings (JSON), e.g. {"claude-sonnet-4-20250514": "claude-sonnet-4"}
#   'p' - Pricing configuration
#   'v' - View supported models
#
"#;

        fs::write(&path, template)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_preserves_comments_removes_on_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        // 写入带注释的模板
        let template = r#"[proxy]
port = 4444                          # Proxy listen port
bind = "127.0.0.1"                   # Bind address

[log]
level = "info"                       # Log level: trace, debug, info, warn, error
"#;
        fs::write(&path, template).unwrap();

        // 模拟 save(): port 改变，其他不变
        let existing_content = fs::read_to_string(&path).unwrap();
        let mut doc: toml_edit::DocumentMut = existing_content.parse().unwrap();

        // 构造新配置 (port=5555, 其他不变)
        let new_toml = r#"[proxy]
port = 5555
bind = "127.0.0.1"

[log]
level = "info"
"#;
        let new_doc: toml_edit::DocumentMut = new_toml.parse().unwrap();

        AppConfig::merge_tables(doc.as_table_mut(), new_doc.as_table());
        let result = doc.to_string();

        println!("=== Result ===\n{}", result);

        // port 值变了 -> 注释应被去掉
        assert!(result.contains("port = 5555"));
        assert!(!result.contains("Proxy listen port"));

        // bind 值未变 -> 注释应保留
        assert!(result.contains("# Bind address"));

        // log.level 未变 -> 注释应保留
        assert!(result.contains("# Log level"));
    }
}
