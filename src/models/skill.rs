// Skill 数据模型

use serde::{Deserialize, Serialize};

/// Skill 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: i64,
    pub name: String,
    pub repo_id: i64,
    pub path: String,
    pub description: Option<String>,
    pub enabled_claude: bool,
    pub enabled_codex: bool,
    pub enabled_gemini: bool,
    pub enabled_opencode: bool,
}

impl Skill {
    pub fn new(name: String, repo_id: i64, path: String) -> Self {
        Self {
            id: 0,
            name,
            repo_id,
            path,
            description: None,
            enabled_claude: false,
            enabled_codex: false,
            enabled_gemini: false,
            enabled_opencode: false,
        }
    }

    pub fn is_enabled_for(&self, app_type: &super::AppType) -> bool {
        match app_type {
            super::AppType::ClaudeCode => self.enabled_claude,
            super::AppType::Codex => self.enabled_codex,
            super::AppType::GeminiCli => self.enabled_gemini,
            super::AppType::OpenCode => self.enabled_opencode,
            super::AppType::OpenClaw => false,
        }
    }

    pub fn set_enabled_for(&mut self, app_type: &super::AppType, enabled: bool) {
        match app_type {
            super::AppType::ClaudeCode => self.enabled_claude = enabled,
            super::AppType::Codex => self.enabled_codex = enabled,
            super::AppType::GeminiCli => self.enabled_gemini = enabled,
            super::AppType::OpenCode => self.enabled_opencode = enabled,
            super::AppType::OpenClaw => {} // not supported
        }
    }

    /// 返回启用此 skill 的工具列表
    pub fn enabled_tools(&self) -> Vec<&'static str> {
        let mut tools = Vec::new();
        if self.enabled_claude { tools.push("Claude"); }
        if self.enabled_codex { tools.push("Codex"); }
        if self.enabled_gemini { tools.push("Gemini"); }
        if self.enabled_opencode { tools.push("OpenCode"); }
        tools
    }
}

/// Skill Repository
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRepo {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub branch: String,
    pub local_path: String,
    pub last_synced: Option<String>,
}

impl SkillRepo {
    pub fn new(name: String, url: String) -> Self {
        Self {
            id: 0,
            name,
            url,
            branch: "main".to_string(),
            local_path: String::new(),
            last_synced: None,
        }
    }
}
