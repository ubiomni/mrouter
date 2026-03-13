// MCP Server 数据模型

use serde::{Deserialize, Serialize};

/// MCP Server 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub id: i64,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Option<serde_json::Value>,
    pub enabled_claude: bool,
    pub enabled_codex: bool,
    pub enabled_gemini: bool,
    pub enabled_opencode: bool,
    pub enabled_openclaw: bool,
}

impl McpServer {
    pub fn new(name: String, command: String) -> Self {
        Self {
            id: 0,
            name,
            command,
            args: Vec::new(),
            env: None,
            enabled_claude: false,
            enabled_codex: false,
            enabled_gemini: false,
            enabled_opencode: false,
            enabled_openclaw: false,
        }
    }

    pub fn is_enabled_for(&self, app_type: &super::AppType) -> bool {
        match app_type {
            super::AppType::ClaudeCode => self.enabled_claude,
            super::AppType::Codex => self.enabled_codex,
            super::AppType::GeminiCli => self.enabled_gemini,
            super::AppType::OpenCode => self.enabled_opencode,
            super::AppType::OpenClaw => self.enabled_openclaw,
        }
    }

    pub fn set_enabled_for(&mut self, app_type: &super::AppType, enabled: bool) {
        match app_type {
            super::AppType::ClaudeCode => self.enabled_claude = enabled,
            super::AppType::Codex => self.enabled_codex = enabled,
            super::AppType::GeminiCli => self.enabled_gemini = enabled,
            super::AppType::OpenCode => self.enabled_opencode = enabled,
            super::AppType::OpenClaw => self.enabled_openclaw = enabled,
        }
    }
}
