// 数据模型定义

use serde::{Deserialize, Serialize};

pub mod provider;
pub mod mcp;
pub mod skill;
pub mod stats;

pub use provider::*;
pub use mcp::*;
pub use skill::*;
pub use stats::*;

/// CLI 工具类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppType {
    ClaudeCode,
    Codex,
    GeminiCli,
    OpenCode,
    OpenClaw,
}

impl AppType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AppType::ClaudeCode => "claude-code",
            AppType::Codex => "codex",
            AppType::GeminiCli => "gemini-cli",
            AppType::OpenCode => "opencode",
            AppType::OpenClaw => "openclaw",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            AppType::ClaudeCode => "Claude Code",
            AppType::Codex => "Codex",
            AppType::GeminiCli => "Gemini CLI",
            AppType::OpenCode => "OpenCode",
            AppType::OpenClaw => "OpenClaw",
        }
    }

    pub fn all() -> Vec<AppType> {
        vec![
            AppType::ClaudeCode,
            AppType::Codex,
            AppType::GeminiCli,
            AppType::OpenCode,
            AppType::OpenClaw,
        ]
    }
}

impl std::fmt::Display for AppType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl std::str::FromStr for AppType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "claude-code" | "claude" => Ok(AppType::ClaudeCode),
            "codex" => Ok(AppType::Codex),
            "gemini-cli" | "gemini" => Ok(AppType::GeminiCli),
            "opencode" => Ok(AppType::OpenCode),
            "openclaw" => Ok(AppType::OpenClaw),
            _ => Err(format!("Unknown app type: {}", s)),
        }
    }
}
