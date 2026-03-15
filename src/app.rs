// 应用状态管理

use anyhow::Result;
use crate::config::AppConfig;
use crate::database::Database;
use crate::database::dao::*;
use crate::models::*;
use crate::tui::widgets::dialog::{DialogKind, InputField, FieldKind};
use crate::services::{SkillService, ProviderSwitchService};
use std::time::Instant;
use std::fs;

pub struct App {
    pub db: Database,
    pub config: AppConfig,
    pub current_tab: Tab,

    // Providers (全局，不按 CLI Tool 分组)
    pub providers: Vec<Provider>,
    pub selected_provider: usize,

    // MCP
    pub mcp_servers: Vec<McpServer>,
    pub selected_mcp: usize,

    // Skills
    pub skills: Vec<Skill>,
    pub skill_repos: Vec<SkillRepo>,
    pub selected_skill: usize,

    // Proxy
    pub proxy_running: bool,
    pub proxy_bind: String,
    pub proxy_port: u16,
    pub proxy_request_count: u64,

    // Health
    pub health_statuses: Vec<(String, ProviderHealth)>,

    // Settings
    pub settings_selected: usize,
    pub input_mode: InputMode,
    pub notification: Option<Notification>,
    pub dialog: Option<DialogKind>,
    pub previous_dialog: Option<DialogKind>, // 保存之前的对话框状态（用于模型浏览窗口返回）
    pub show_help: bool,

    // Stats
    pub stats_summary: Option<crate::models::UsageSummary>,
    pub stats_time_range: StatsTimeRange,

    // Request Logs
    pub request_logs: Vec<ProxyRequestLog>,
    pub selected_log: usize,
    pub logs_page: usize,
    pub logs_per_page: usize,
    pub show_log_detail: bool,
    pub log_detail_scroll: usize,  // 详情页滚动偏移量

    // Sync mode selection
    pub pending_sync_cli_tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
    Providers,
    Mcp,
    Stats,  // 统计信息
    Proxy,
    RequestLogs,  // 请求日志
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatsTimeRange {
    Today,
    Week,
    Month,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
    Searching,
}

pub struct Notification {
    pub message: String,
    pub level: NotificationLevel,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Copy)]
pub enum NotificationLevel {
    Success,
    Warning,
    Error,
    Info,
}

impl App {
    pub async fn new(db: Database, config: AppConfig) -> Result<Self> {
        // 获取所有 Provider（不按 CLI Tool 过滤）
        let mut all_providers = Vec::new();
        for app_type in AppType::all() {
            let mut providers = ProviderDao::get_all(&db, app_type)?;
            all_providers.append(&mut providers);
        }

        let mcp_servers = McpDao::get_all(&db)?;
        let skills = SkillDao::get_all(&db)?;
        let skill_repos = SkillRepoDao::get_all(&db)?;

        // 从配置文件读取 proxy 配置
        let proxy_bind = config.proxy.bind.clone();
        let proxy_port = config.proxy.port;

        // 检查 proxy 是否在运行
        let proxy_running = Self::check_daemon_running().await;

        Ok(Self {
            db,
            config,
            current_tab: Tab::Providers,
            providers: all_providers,
            selected_provider: 0,
            mcp_servers,
            selected_mcp: 0,
            skills,
            skill_repos,
            selected_skill: 0,
            proxy_running,
            proxy_bind,
            proxy_port,
            proxy_request_count: 0,
            health_statuses: Vec::new(),
            settings_selected: 0,
            input_mode: InputMode::Normal,
            notification: None,
            dialog: None,
            previous_dialog: None,
            show_help: false,
            stats_summary: None,
            stats_time_range: StatsTimeRange::Today,
            request_logs: Vec::new(),
            selected_log: 0,
            logs_page: 0,
            logs_per_page: 50,
            show_log_detail: false,
            log_detail_scroll: 0,
            pending_sync_cli_tools: None,
        })
    }

    /// 检查 daemon 是否在运行
    async fn check_daemon_running() -> bool {
        // 直接检查 PID 文件和进程是否存在
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return false,
        };
        let pid_file = home.join(".mrouter").join("daemon.pid");

        if !pid_file.exists() {
            return false;
        }

        // 读取 PID
        let pid_str = match std::fs::read_to_string(&pid_file) {
            Ok(s) => s,
            Err(_) => return false,
        };

        let pid: u32 = match pid_str.trim().parse() {
            Ok(p) => p,
            Err(_) => return false,
        };

        // 使用 kill -0 检查进程是否存在
        #[cfg(unix)]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .output()
            {
                return output.status.success();
            }
        }

        false
    }
    
    pub async fn refresh(&mut self) -> Result<()> {
        // 获取所有 Provider（不按 CLI Tool 过滤）
        let mut all_providers = Vec::new();
        for app_type in AppType::all() {
            let mut providers = ProviderDao::get_all(&self.db, app_type)?;
            all_providers.append(&mut providers);
        }
        self.providers = all_providers;

        self.mcp_servers = McpDao::get_all(&self.db)?;
        self.skills = SkillDao::get_all(&self.db)?;
        self.skill_repos = SkillRepoDao::get_all(&self.db)?;

        // 检查 proxy 状态
        self.check_proxy_status().await?;

        // 如果在 Stats 标签页,刷新统计数据
        if self.current_tab == Tab::Stats {
            self.refresh_stats()?;
        }

        // 如果在 RequestLogs 标签页,刷新请求日志
        if self.current_tab == Tab::RequestLogs {
            self.refresh_request_logs()?;
        }

        // 显示刷新成功通知
        self.show_notification("Refreshed".to_string(), NotificationLevel::Success);

        Ok(())
    }

    /// 刷新统计数据
    pub fn refresh_stats(&mut self) -> Result<()> {
        use crate::database::dao::StatsDao;
        use chrono::{Utc, Duration};

        let (from, to) = match self.stats_time_range {
            StatsTimeRange::Today => {
                let now = Utc::now();
                let start_of_day = now.date_naive().and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_local_timezone(Utc)
                    .unwrap();
                (start_of_day, now)
            }
            StatsTimeRange::Week => {
                let now = Utc::now();
                (now - Duration::days(7), now)
            }
            StatsTimeRange::Month => {
                let now = Utc::now();
                (now - Duration::days(30), now)
            }
            StatsTimeRange::All => {
                let now = Utc::now();
                (now - Duration::days(365 * 10), now)  // 10 years ago
            }
        };

        self.stats_summary = Some(StatsDao::get_summary(&self.db, from, to)?);
        Ok(())
    }

    /// 刷新请求日志
    pub fn refresh_request_logs(&mut self) -> Result<()> {
        use crate::database::dao::StatsDao;

        // 获取最近的日志（分页）
        let limit = self.logs_per_page as i64;
        self.request_logs = StatsDao::get_recent_request_logs(&self.db, None, limit)?;

        // 重置选择
        if self.selected_log >= self.request_logs.len() {
            self.selected_log = 0;
        }

        Ok(())
    }

    /// 获取选中的请求日志
    pub fn get_selected_log(&self) -> Option<&ProxyRequestLog> {
        self.request_logs.get(self.selected_log)
    }

    /// 根据 provider_id 获取 provider 名称
    pub fn get_provider_name(&self, provider_id: i64) -> String {
        self.providers
            .iter()
            .find(|p| p.id == provider_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| format!("Provider#{}", provider_id))
    }

    /// 下一条日志
    pub fn next_log(&mut self) {
        if !self.request_logs.is_empty() {
            self.selected_log = (self.selected_log + 1) % self.request_logs.len();
        }
    }

    /// 上一条日志
    pub fn previous_log(&mut self) {
        if !self.request_logs.is_empty() {
            if self.selected_log == 0 {
                self.selected_log = self.request_logs.len() - 1;
            } else {
                self.selected_log -= 1;
            }
        }
    }

    /// 切换日志详情显示
    pub fn toggle_log_detail(&mut self) {
        self.show_log_detail = !self.show_log_detail;
        // 重置滚动位置
        if self.show_log_detail {
            self.log_detail_scroll = 0;
        }
    }

    /// 日志详情向上滚动
    pub fn scroll_log_detail_up(&mut self) {
        if self.log_detail_scroll > 0 {
            self.log_detail_scroll -= 1;
        }
    }

    /// 日志详情向下滚动
    pub fn scroll_log_detail_down(&mut self) {
        self.log_detail_scroll += 1;
    }

    /// 下一页日志
    pub fn next_logs_page(&mut self) -> Result<()> {
        use crate::database::dao::StatsDao;

        // 获取总日志数
        let total_logs = StatsDao::count_request_logs(&self.db, None)?;
        let total_pages = (total_logs as f64 / self.logs_per_page as f64).ceil() as usize;

        if self.logs_page + 1 < total_pages {
            self.logs_page += 1;
            self.refresh_request_logs_with_offset()?;
        }

        Ok(())
    }

    /// 上一页日志
    pub fn previous_logs_page(&mut self) -> Result<()> {
        if self.logs_page > 0 {
            self.logs_page -= 1;
            self.refresh_request_logs_with_offset()?;
        }

        Ok(())
    }

    /// 刷新日志（带分页偏移）
    fn refresh_request_logs_with_offset(&mut self) -> Result<()> {
        use crate::database::dao::StatsDao;

        let offset = (self.logs_page * self.logs_per_page) as i64;
        let limit = self.logs_per_page as i64;

        self.request_logs = StatsDao::get_recent_request_logs_with_offset(&self.db, None, limit, offset)?;

        // 重置选择
        self.selected_log = 0;

        Ok(())
    }

    /// 获取总页数
    pub fn get_total_logs_pages(&self) -> Result<usize> {
        use crate::database::dao::StatsDao;

        let total_logs = StatsDao::count_request_logs(&self.db, None)?;
        let total_pages = (total_logs as f64 / self.logs_per_page as f64).ceil() as usize;

        Ok(total_pages.max(1))
    }
    
    pub fn navigate(&mut self, dir: crate::tui::event::Direction) {
        use crate::tui::event::Direction;
        
        match self.current_tab {
            Tab::Providers => match dir {
                Direction::Up => {
                    if self.selected_provider > 0 {
                        self.selected_provider -= 1;
                    }
                }
                Direction::Down => {
                    if self.selected_provider < self.providers.len().saturating_sub(1) {
                        self.selected_provider += 1;
                    }
                }
                _ => {}
            },
            Tab::Mcp => match dir {
                Direction::Up => {
                    if self.selected_mcp > 0 {
                        self.selected_mcp -= 1;
                    }
                }
                Direction::Down => {
                    if self.selected_mcp < self.mcp_servers.len().saturating_sub(1) {
                        self.selected_mcp += 1;
                    }
                }
                _ => {}
            },
            Tab::Stats => match dir {
                Direction::Up => {
                    if self.selected_skill > 0 {
                        self.selected_skill -= 1;
                    }
                }
                Direction::Down => {
                    if self.selected_skill < self.skills.len().saturating_sub(1) {
                        self.selected_skill += 1;
                    }
                }
                _ => {}
            },
            Tab::Settings => match dir {
                Direction::Up => {
                    if self.settings_selected > 0 {
                        self.settings_selected -= 1;
                        // 跳过分类标题行 (0, 5, 11, 15, 19, 21, 26)
                        let category_headers = vec![0, 5, 11, 15, 19, 21, 26];
                        while category_headers.contains(&self.settings_selected) && self.settings_selected > 0 {
                            self.settings_selected -= 1;
                        }
                    }
                }
                Direction::Down => {
                    // 总共 28 项 (包括 7 个分类标题)
                    if self.settings_selected < 27 {
                        self.settings_selected += 1;
                        // 跳过分类标题行
                        let category_headers = vec![0, 5, 11, 15, 19, 21, 26];
                        while category_headers.contains(&self.settings_selected) && self.settings_selected < 27 {
                            self.settings_selected += 1;
                        }
                    }
                }
                _ => {}
            },
            Tab::RequestLogs => match dir {
                Direction::Up => {
                    self.previous_log();
                }
                Direction::Down => {
                    self.next_log();
                }
                _ => {}
            },
            _ => {}
        }
    }
    
    pub async fn handle_select(&mut self) -> Result<()> {
        match self.current_tab {
            Tab::Providers => {
                if let Some(provider) = self.providers.get(self.selected_provider) {
                    let name = provider.name.clone();
                    let current_active = provider.is_active;

                    // Toggle 激活状态（支持多个 Provider 同时激活，用于 Proxy 故障转移）
                    // 注意：切换激活状态时不同步配置，只有用户按 's' 明确选择同步时才同步
                    let mut updated_provider = provider.clone();
                    updated_provider.is_active = !current_active;
                    ProviderDao::update(&self.db, &updated_provider)?;

                    let status = if updated_provider.is_active { "Activated" } else { "Deactivated" };
                    self.show_notification(
                        format!("{}: {}", status, name),
                        NotificationLevel::Success,
                    );
                    self.refresh().await?;
                }
            }
            Tab::Mcp => {
                // 显示 MCP 服务器的 CLI Tool 选择对话框
                if let Some(server) = self.mcp_servers.get(self.selected_mcp) {
                    self.show_mcp_cli_tool_dialog(server.clone());
                }
            }
            Tab::Stats => {
                // 显示 Skill 的 CLI Tool 选择对话框
                if let Some(skill) = self.skills.get(self.selected_skill) {
                    self.show_skill_cli_tool_dialog(skill.clone());
                }
            }
            _ => {}
        }
        Ok(())
    }
    
    pub async fn handle_edit(&mut self) -> Result<()> {
        match self.current_tab {
            Tab::Providers => {
                if let Some(provider) = self.providers.get(self.selected_provider) {
                    let type_options: Vec<String> = crate::models::ProviderType::all()
                        .iter().map(|t| t.display_name().to_string()).collect();
                    let current_idx = crate::models::ProviderType::all()
                        .iter().position(|t| *t == provider.provider_type).unwrap_or(0);

                    let mut fields = vec![
                        InputField::select("Type", type_options, current_idx),
                        InputField::new("Name", ""),
                        InputField::password("API Key", ""),
                        InputField::new("Base URL", ""),
                        InputField::new("Model", ""),
                        InputField::new("Priority", "0"),
                        InputField::new("Supported Models", ""),
                        InputField::select("API Format", vec!["Auto".into(), "Anthropic".into(), "OpenAI".into(), "Google".into()], 0),
                        InputField::checkbox("Enable Token Stats", false),
                    ];

                    // 预填充现有值
                    fields[1].set_value(provider.name.clone());
                    fields[2].set_value(provider.api_key.clone());
                    fields[3].set_value(provider.base_url.clone());
                    fields[4].set_value(provider.model.clone().unwrap_or_default());
                    fields[5].set_value(provider.priority.to_string());
                    fields[6].set_value(provider.supported_models.as_ref()
                        .map(|models| models.join(", "))
                        .unwrap_or_default());
                    // API Format: 预填充
                    let api_format_idx = match provider.api_format {
                        None => 0,  // Auto
                        Some(crate::models::ApiFormat::Anthropic) => 1,
                        Some(crate::models::ApiFormat::OpenAI) => 2,
                        Some(crate::models::ApiFormat::Google) => 3,
                    };
                    fields[7] = InputField::select("API Format", vec!["Auto".into(), "Anthropic".into(), "OpenAI".into(), "Google".into()], api_format_idx);
                    fields[8] = InputField::checkbox("Enable Token Stats", provider.enable_stats);

                    self.dialog = Some(DialogKind::Input {
                        title: "Edit Provider".to_string(),
                        fields,
                        focused_field: 0,
                    });
                    self.input_mode = InputMode::Editing;
                }
            }
            Tab::Settings => {
                self.open_settings_edit_dialog();
            }
            Tab::Stats => {
                if let Some(skill) = self.skills.get(self.selected_skill) {
                    let mut fields = vec![
                        InputField::new("Name", "Skill name"),
                        InputField::new("Description", "What this skill does"),
                    ];

                    fields[0].set_value(skill.name.clone());
                    fields[1].set_value(skill.description.clone().unwrap_or_default());

                    self.dialog = Some(DialogKind::Input {
                        title: "Edit Skill".to_string(),
                        fields,
                        focused_field: 0,
                    });
                    self.input_mode = InputMode::Editing;
                }
            }
            _ => {}
        }
        Ok(())
    }
    
    pub async fn handle_delete(&mut self) -> Result<()> {
        match self.current_tab {
            Tab::Providers => {
                if let Some(provider) = self.providers.get(self.selected_provider) {
                    if provider.is_active {
                        self.show_notification(
                            "Cannot delete active provider".to_string(),
                            NotificationLevel::Error,
                        );
                        return Ok(());
                    }
                    
                    self.dialog = Some(DialogKind::Confirm {
                        title: "Delete Provider".to_string(),
                        message: format!("Delete provider \"{}\"?\nThis cannot be undone.", provider.name),
                    });
                }
            }
            Tab::Mcp => {
                if let Some(server) = self.mcp_servers.get(self.selected_mcp) {
                    self.dialog = Some(DialogKind::Confirm {
                        title: "Delete MCP Server".to_string(),
                        message: format!("Delete MCP server \"{}\"?", server.name),
                    });
                }
            }
            Tab::Stats => {
                if let Some(skill) = self.skills.get(self.selected_skill) {
                    self.dialog = Some(DialogKind::Confirm {
                        title: "Delete Skill".to_string(),
                        message: format!("Delete skill \"{}\"?", skill.name),
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }
    
    pub async fn handle_add(&mut self) -> Result<()> {
        match self.current_tab {
            Tab::Providers => {
                let type_options: Vec<String> = crate::models::ProviderType::all()
                    .iter().map(|t| t.display_name().to_string()).collect();

                let mut fields = vec![
                    InputField::select("Type", type_options, 0),
                    InputField::new("Name", "My Anthropic Key"),
                    InputField::password("API Key", "sk-xxx..."),
                    InputField::new("Base URL", "(auto-fill from type)"),
                    InputField::new("Model", ""),
                    InputField::new("Priority", "0 (lower = higher priority)"),
                    InputField::new("Supported Models", "gpt-4, gpt-4-turbo, gpt-3.5-turbo (comma-separated, optional)"),
                    InputField::select("API Format", vec!["Auto".into(), "Anthropic".into(), "OpenAI".into(), "Google".into()], 0),
                    InputField::checkbox("Enable Token Stats", false),
                ];

                // 自动填充默认 provider type (Anthropic) 的 base_url 和 model
                if let Ok(provider_type) = fields[0].value.parse::<crate::models::ProviderType>() {
                    let default_base_url = provider_type.default_base_url();
                    if !default_base_url.is_empty() {
                        fields[3].set_value(default_base_url.to_string());
                    }

                    // Model: 从 supported_models 取第一个
                    let models = provider_type.default_supported_models();
                    if !models.is_empty() {
                        fields[4].set_value(models[0].clone());
                        fields[6].set_value(models.join(", "));
                        fields[6].placeholder = "(auto-filled, press Ctrl+F to fetch latest)".to_string();
                    }
                }

                self.dialog = Some(DialogKind::Input {
                    title: "Add Provider (Proxy Mode)".to_string(),
                    fields,
                    focused_field: 0,
                });
                self.input_mode = InputMode::Editing;
            }
            Tab::Mcp => {
                let fields = vec![
                    InputField::new("Name", "Server name"),
                    InputField::new("Command", "npx"),
                    InputField::new("Args", "[@scope/server-name]"),
                ];

                self.dialog = Some(DialogKind::Input {
                    title: "Add MCP Server".to_string(),
                    fields,
                    focused_field: 0,
                });
                self.input_mode = InputMode::Editing;
            }
            Tab::Stats => {
                let fields = vec![
                    InputField::new("Name", "repo-name"),
                    InputField::new("URL", "https://github.com/user/skills-repo"),
                    InputField::new("Branch", "main"),
                ];

                self.dialog = Some(DialogKind::Input {
                    title: "Add Skill Repository".to_string(),
                    fields,
                    focused_field: 0,
                });
                self.input_mode = InputMode::Editing;
            }
            _ => {}
        }
        Ok(())
    }
    
    /// 确认对话框操作
    pub async fn handle_confirm_yes(&mut self) -> Result<()> {
        // 检查是否是同步模式选择（Proxy 模式）
        if let Some(cli_tools) = self.pending_sync_cli_tools.take() {
            if let Some(provider) = self.providers.get(self.selected_provider) {
                let provider_id = provider.id;
                let connect_host = if self.proxy_bind == "0.0.0.0" { "127.0.0.1" } else { &self.proxy_bind };
                let proxy_url = Some(format!("http://{}:{}", connect_host, self.proxy_port));

                ProviderSwitchService::set_sync_to_cli_tools(
                    &self.db, provider_id, cli_tools.clone(), proxy_url,
                )?;

                self.show_notification(
                    format!("Synced (Proxy): {}", cli_tools.join(", ")),
                    NotificationLevel::Success,
                );
                self.refresh().await?;
            }
            self.dialog = None;
            return Ok(());
        }

        // 检查是否是重置 Circuit Breaker 的确认对话框
        if let Some(DialogKind::Confirm { title, .. }) = &self.dialog {
            if title.contains("Reset Circuit Breaker") {
                self.dialog = None;
                // 重启 proxy 来重置 circuit breaker
                self.handle_proxy_stop().await?;
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                self.handle_proxy_start().await?;
                self.show_notification(
                    "Circuit breaker reset (proxy restarted)".to_string(),
                    NotificationLevel::Success,
                );
                return Ok(());
            }
        }

        match self.current_tab {
            Tab::Providers => {
                if let Some(provider) = self.providers.get(self.selected_provider) {
                    let name = provider.name.clone();
                    ProviderDao::delete(&self.db, provider.id)?;
                    self.show_notification(
                        format!("Deleted: {}", name),
                        NotificationLevel::Success,
                    );
                    self.refresh().await?;
                    if self.selected_provider > 0 {
                        self.selected_provider -= 1;
                    }
                }
            }
            Tab::Mcp => {
                if let Some(server) = self.mcp_servers.get(self.selected_mcp) {
                    let name = server.name.clone();
                    McpDao::delete(&self.db, server.id)?;
                    self.show_notification(
                        format!("Deleted: {}", name),
                        NotificationLevel::Success,
                    );
                    self.refresh().await?;
                    if self.selected_mcp > 0 {
                        self.selected_mcp -= 1;
                    }
                }
            }
            Tab::Stats => {
                if let Some(skill) = self.skills.get(self.selected_skill) {
                    let name = skill.name.clone();
                    SkillDao::delete(&self.db, skill.id)?;
                    self.show_notification(
                        format!("Deleted skill: {}", name),
                        NotificationLevel::Success,
                    );
                    self.refresh().await?;
                    if self.selected_skill > 0 {
                        self.selected_skill -= 1;
                    }
                }
            }
            _ => {}
        }

        self.dialog = None;
        Ok(())
    }

    /// 同步模式选择：Direct 模式（ConfirmNo 时调用）
    pub async fn handle_sync_direct(&mut self) -> Result<()> {
        if let Some(cli_tools) = self.pending_sync_cli_tools.take() {
            if let Some(provider) = self.providers.get(self.selected_provider) {
                let provider_id = provider.id;

                ProviderSwitchService::set_sync_to_cli_tools(
                    &self.db, provider_id, cli_tools.clone(), None,
                )?;

                self.show_notification(
                    format!("Synced (Direct): {}", cli_tools.join(", ")),
                    NotificationLevel::Success,
                );
                self.refresh().await?;
            }
        }
        self.dialog = None;
        Ok(())
    }

    /// 提交输入对话框
    pub async fn handle_input_submit(&mut self) -> Result<()> {
        // 如果当前聚焦的是 Select 字段，Enter 确认选择并跳到下一个字段
        if let Some(DialogKind::Input { fields, focused_field, title }) = &mut self.dialog {
            // 先获取需要的信息
            let should_autofill = (title.contains("Add Provider") || title.contains("Edit Provider"))
                && *focused_field == 0
                && fields.len() > 6;

            if let Some(field) = fields.get_mut(*focused_field) {
                if field.is_select() {
                    field.select_confirm();

                    // 如果需要自动填充，获取 provider_type 的值
                    if should_autofill {
                        if let Ok(provider_type) = field.value.parse::<crate::models::ProviderType>() {
                            let default_models = provider_type.default_supported_models();
                            if !default_models.is_empty() {
                                // 释放 field 的可变借用后再访问 fields[6]
                                let _ = field;
                                if let Some(models_field) = fields.get_mut(6) {
                                    models_field.value = default_models.join(", ");
                                    models_field.placeholder = "(auto-filled, fetching latest...)".to_string();
                                }

                                *focused_field = (*focused_field + 1) % fields.len();
                                return Ok(());
                            }
                        }
                    }

                    *focused_field = (*focused_field + 1) % fields.len();
                    return Ok(());
                }
            }
        }

        if let Some(DialogKind::Input { title, fields, .. }) = &self.dialog {
            // 先处理特殊对话框（定价配置、模型映射）
            if title.contains("Configure Pricing") {
                // 解析定价字段
                let input_price = fields[0].value.parse::<f64>().unwrap_or(0.0);
                let output_price = fields[1].value.parse::<f64>().unwrap_or(0.0);
                let cache_write_price = fields[2].value.parse::<f64>().unwrap_or(0.0);
                let cache_read_price = fields[3].value.parse::<f64>().unwrap_or(0.0);

                // 更新 Provider 的定价配置
                if let Some(provider) = self.providers.get(self.selected_provider) {
                    let mut updated = provider.clone();

                    let pricing = crate::models::PricingConfig {
                        input_price_per_million: input_price,
                        output_price_per_million: output_price,
                        cache_write_price_per_million: cache_write_price,
                        cache_read_price_per_million: cache_read_price,
                    };

                    // 保存到 config JSON
                    let mut config_map = if let serde_json::Value::Object(map) = updated.config {
                        map
                    } else {
                        serde_json::Map::new()
                    };
                    config_map.insert("pricing".to_string(), serde_json::to_value(&pricing)?);
                    updated.config = serde_json::Value::Object(config_map);

                    ProviderDao::update(&self.db, &updated)?;
                    self.show_notification(
                        format!("Updated pricing for: {}", updated.name),
                        NotificationLevel::Success
                    );
                    self.refresh().await?;
                }

                self.dialog = None;
                self.input_mode = InputMode::Normal;
                return Ok(());
            } else if title.contains("Configure Model Mappings") {
                // 解析模型映射字段
                let mappings_str = fields[0].value.trim();

                // 更新 Provider 的模型映射配置
                if let Some(provider) = self.providers.get(self.selected_provider) {
                    let mut updated = provider.clone();

                    // 解析映射字符串：alias1=actual1, alias2=actual2
                    let mut mappings = std::collections::HashMap::new();
                    if !mappings_str.is_empty() {
                        for pair in mappings_str.split(',') {
                            let parts: Vec<&str> = pair.trim().split('=').collect();
                            if parts.len() == 2 {
                                let alias = parts[0].trim().to_string();
                                let actual = parts[1].trim().to_string();
                                if !alias.is_empty() && !actual.is_empty() {
                                    mappings.insert(alias, actual);
                                }
                            }
                        }
                    }

                    // 保存到 config JSON
                    let mut config_map = if let serde_json::Value::Object(map) = updated.config {
                        map
                    } else {
                        serde_json::Map::new()
                    };

                    if mappings.is_empty() {
                        // 如果映射为空，移除配置
                        config_map.remove("model_mappings");
                    } else {
                        config_map.insert("model_mappings".to_string(), serde_json::to_value(&mappings)?);
                    }

                    updated.config = serde_json::Value::Object(config_map);

                    ProviderDao::update(&self.db, &updated)?;
                    self.show_notification(
                        format!("Updated model mappings for: {} ({} mappings)", updated.name, mappings.len()),
                        NotificationLevel::Success
                    );
                    self.refresh().await?;
                }

                self.dialog = None;
                self.input_mode = InputMode::Normal;
                return Ok(());
            } else if title.contains("Configure Headers") {
                let auth_spec = fields[0].value.trim().to_string();
                let headers_json = fields[1].value.trim().to_string();

                if let Some(provider) = self.providers.get(self.selected_provider) {
                    let mut updated = provider.clone();

                    let mut config_map = if let serde_json::Value::Object(map) = updated.config {
                        map
                    } else {
                        serde_json::Map::new()
                    };

                    // Auth Header
                    if auth_spec.is_empty() || auth_spec == provider.provider_type.default_auth_header_spec() {
                        config_map.remove("auth_header");
                    } else {
                        config_map.insert("auth_header".to_string(), serde_json::Value::String(auth_spec));
                    }

                    // Custom Headers
                    if headers_json.is_empty() || headers_json == "{}" {
                        config_map.remove("custom_headers");
                    } else {
                        match serde_json::from_str::<serde_json::Value>(&headers_json) {
                            Ok(val) if val.is_object() => {
                                config_map.insert("custom_headers".to_string(), val);
                            }
                            _ => {
                                self.show_notification(
                                    r#"Invalid JSON. Example: {"User-Agent": "my-agent/1.0"}"#.to_string(),
                                    NotificationLevel::Error
                                );
                                return Ok(());
                            }
                        }
                    }

                    updated.config = serde_json::Value::Object(config_map);
                    ProviderDao::update(&self.db, &updated)?;
                    self.show_notification(
                        format!("Updated headers for: {}", updated.name),
                        NotificationLevel::Success
                    );
                    self.refresh().await?;
                }

                self.dialog = None;
                self.input_mode = InputMode::Normal;
                return Ok(());
            }

            // 处理普通的 Add/Edit Provider 对话框
            match self.current_tab {
                Tab::Providers => {
                    let type_str = fields[0].value.clone();
                    let name = fields[1].value.clone();
                    let api_key = fields[2].value.clone();
                    let base_url = fields[3].value.clone();
                    // Model: 为空时设为 None（透传模式）
                    let model = if fields[4].value.is_empty() {
                        None
                    } else {
                        Some(fields[4].value.clone())
                    };
                    // Priority: 如果为空，使用 placeholder 或默认值 0
                    let priority = if fields[5].value.is_empty() {
                        fields[5].placeholder.parse::<i32>().unwrap_or(0)
                    } else {
                        fields[5].value.parse::<i32>().unwrap_or(0)
                    };
                    // Supported Models: 解析逗号分隔的字符串
                    let supported_models = if fields.len() > 6 && !fields[6].value.is_empty() {
                        let models: Vec<String> = fields[6].value
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        if models.is_empty() {
                            None
                        } else {
                            Some(models)
                        }
                    } else {
                        None
                    };
                    // Enable Stats: 从 checkbox 字段读取
                    let enable_stats = if fields.len() > 8 {
                        fields[8].value == "true"
                    } else {
                        false  // 兜底：无 checkbox 字段时默认不启用
                    };
                    // API Format: 从 select 字段读取
                    let api_format = if fields.len() > 7 {
                        match fields[7].value.as_str() {
                            "Anthropic" => Some(crate::models::ApiFormat::Anthropic),
                            "OpenAI" => Some(crate::models::ApiFormat::OpenAI),
                            "Google" => Some(crate::models::ApiFormat::Google),
                            _ => None, // "Auto" or anything else
                        }
                    } else {
                        None
                    };

                    let provider_type = type_str.parse::<crate::models::ProviderType>()
                        .unwrap_or(crate::models::ProviderType::Custom);

                    // Auto-fill base_url and model from provider type defaults if empty
                    let base_url = if base_url.is_empty() || base_url.contains("auto-fill") {
                        provider_type.default_base_url().to_string()
                    } else {
                        base_url
                    };
                    // Model: 保持用户的选择，不自动回填默认值
                    // 为空时表示透传模式

                    if name.is_empty() || api_key.is_empty() || base_url.is_empty() {
                        self.show_notification("Name, API Key and Base URL are required".to_string(), NotificationLevel::Error);
                        return Ok(());
                    }

                    // Provider 名称：字母开头，后续只允许字母、数字、下划线（用于生成 env 变量名和 JSON key）
                    if !name.starts_with(|c: char| c.is_ascii_alphabetic())
                        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        self.show_notification("Name must start with a letter, and contain only letters, digits, underscores".to_string(), NotificationLevel::Error);
                        return Ok(());
                    }

                    if title.contains("Add") {
                        // 默认使用 ClaudeCode，但 sync_to_cli_tools 为空（纯代理模式）
                        let mut provider = Provider::new(AppType::ClaudeCode, provider_type, name.clone(), api_key, base_url);
                        provider.model = model;
                        provider.priority = priority;
                        provider.supported_models = supported_models;
                        provider.enable_stats = enable_stats;
                        provider.api_format = api_format;

                        // sync_to_cli_tools 默认为空，表示纯代理模式
                        ProviderDao::insert(&self.db, &provider)?;
                        self.show_notification(
                            format!("Added: {} (Proxy Mode, Priority: {})", name, priority),
                            NotificationLevel::Success
                        );
                    } else {
                        // Edit
                        if let Some(existing) = self.providers.get(self.selected_provider) {
                            let mut updated = existing.clone();
                            updated.provider_type = provider_type;
                            updated.name = name.clone();
                            updated.api_key = api_key;
                            updated.base_url = base_url;
                            updated.model = model;
                            updated.priority = priority;
                            updated.supported_models = supported_models;
                            updated.enable_stats = enable_stats;
                            updated.api_format = api_format;

                            ProviderDao::update(&self.db, &updated)?;
                            self.show_notification(format!("Updated: {}", name), NotificationLevel::Success);
                        }
                    }

                    self.refresh().await?;
                }
                Tab::Mcp => {
                    let name = fields[0].value.clone();
                    let command = fields[1].value.clone();
                    let args_str = fields[2].value.clone();

                    if name.is_empty() || command.is_empty() {
                        self.show_notification("Name and command are required".to_string(), NotificationLevel::Error);
                        return Ok(());
                    }

                    let args: Vec<String> = if args_str.is_empty() {
                        Vec::new()
                    } else {
                        args_str.split(',').map(|s| s.trim().to_string()).collect()
                    };

                    let mut server = McpServer::new(name.clone(), command);
                    server.args = args;
                    McpDao::insert(&self.db, &server)?;
                    self.show_notification(format!("Added MCP: {}", name), NotificationLevel::Success);
                    self.refresh().await?;
                }
                Tab::Stats => {
                    if title.contains("Repository") {
                        // Add Skill Repository
                        let name = fields[0].value.clone();
                        let url = fields[1].value.clone();
                        let branch = if fields[2].value.is_empty() { "main".to_string() } else { fields[2].value.clone() };

                        if name.is_empty() || url.is_empty() {
                            self.show_notification("Name and URL are required".to_string(), NotificationLevel::Error);
                            return Ok(());
                        }

                        self.show_notification(format!("Cloning {}...", name), NotificationLevel::Info);

                        match SkillService::add_repo(&self.db, &name, &url, &branch) {
                            Ok(repo) => {
                                let skill_count = SkillDao::get_by_repo(&self.db, repo.id)?.len();
                                self.show_notification(
                                    format!("Added repo: {} ({} skills found)", name, skill_count),
                                    NotificationLevel::Success,
                                );
                            }
                            Err(e) => {
                                self.show_notification(
                                    format!("Failed to clone: {}", e),
                                    NotificationLevel::Error,
                                );
                            }
                        }
                        self.refresh().await?;
                    } else {
                        // Edit Skill
                        let name = fields[0].value.clone();
                        let desc = fields[1].value.clone();

                        if let Some(existing) = self.skills.get(self.selected_skill) {
                            let mut updated = existing.clone();
                            updated.name = name.clone();
                            updated.description = if desc.is_empty() { None } else { Some(desc) };
                            SkillDao::update(&self.db, &updated)?;
                            self.show_notification(format!("Updated: {}", name), NotificationLevel::Success);
                        }
                        self.refresh().await?;
                    }
                }
                Tab::Settings => {
                    // 使用 placeholder 作为默认值（如果输入为空）
                    let value = if fields[0].value.is_empty() {
                        fields[0].placeholder.clone()
                    } else {
                        fields[0].value.clone()
                    };
                    self.save_settings_edit(&value)?;
                }
                Tab::Proxy => {
                    // 编辑 Proxy 配置
                    if title.contains("Config") {
                        // 使用 placeholder 作为默认值（如果输入为空）
                        let bind_str = if fields[0].value.is_empty() {
                            fields[0].placeholder.clone()
                        } else {
                            fields[0].value.clone()
                        };
                        let port_str = if fields[1].value.is_empty() {
                            fields[1].placeholder.clone()
                        } else {
                            fields[1].value.clone()
                        };

                        // 验证 bind 地址
                        if bind_str.is_empty() {
                            self.show_notification(
                                "Bind address cannot be empty".to_string(),
                                NotificationLevel::Error,
                            );
                            return Ok(());
                        }

                        // 验证端口
                        let port = match port_str.parse::<u16>() {
                            Ok(p) => p,
                            Err(_) => {
                                self.show_notification(
                                    "Invalid port number".to_string(),
                                    NotificationLevel::Error,
                                );
                                return Ok(());
                            }
                        };

                        // 检查端口范围
                        if port < 1024 {
                            self.show_notification(
                                "Port must be >= 1024 (privileged ports require root)".to_string(),
                                NotificationLevel::Error,
                            );
                            return Ok(());
                        }

                        // 检查端口是否被占用（如果 proxy 未运行）
                        if !self.proxy_running && self.is_port_in_use(port).await {
                            self.show_notification(
                                format!("Port {} is already in use. Choose another port.", port),
                                NotificationLevel::Error,
                            );
                            return Ok(());
                        }

                        // 更新配置文件
                        let mut config = self.config.clone();
                        config.proxy.bind = bind_str.clone();
                        config.proxy.port = port;
                        config.save()?;

                        self.proxy_bind = bind_str;
                        self.proxy_port = port;

                        let msg = if self.proxy_running {
                            "Config updated. Restart proxy to apply changes.".to_string()
                        } else {
                            "Config updated.".to_string()
                        };

                        self.show_notification(msg, NotificationLevel::Success);
                    }
                }
                Tab::RequestLogs => {
                    // RequestLogs tab doesn't have add/edit functionality
                    // Just close the dialog
                }
                _ => {}
            }
        }

        self.dialog = None;
        self.input_mode = InputMode::Normal;
        Ok(())
    }

    /// 管理 Provider 的同步设置
    pub async fn manage_sync_settings(&mut self) -> Result<()> {
        if self.current_tab != Tab::Providers {
            return Ok(());
        }

        if let Some(provider) = self.providers.get(self.selected_provider) {
            let cli_tools = AppType::all();
            let mut options = Vec::new();
            let mut selected_indices = Vec::new();

            for (idx, cli_tool) in cli_tools.iter().enumerate() {
                options.push(cli_tool.display_name().to_string());
                if provider.should_sync_to(cli_tool.as_str()) {
                    selected_indices.push(idx);
                }
            }

            // 创建多选对话框
            self.dialog = Some(DialogKind::MultiSelect {
                title: format!("Sync Settings: {}", provider.name),
                message: "Select CLI Tools to sync this provider to:\n(Empty = Proxy-only mode)".to_string(),
                options,
                selected: selected_indices,
                highlighted: 0,
            });
        }

        Ok(())
    }

    /// 显示 MCP 服务器的 CLI Tool 选择对话框
    fn show_mcp_cli_tool_dialog(&mut self, server: McpServer) {
        let cli_tools = AppType::all();
        let mut options = Vec::new();
        let mut selected_indices = Vec::new();

        for (idx, cli_tool) in cli_tools.iter().enumerate() {
            options.push(cli_tool.display_name().to_string());
            if server.is_enabled_for(cli_tool) {
                selected_indices.push(idx);
            }
        }

        self.dialog = Some(DialogKind::MultiSelect {
            title: format!("Enable MCP: {}", server.name),
            message: "Select CLI Tools to enable this MCP server for:".to_string(),
            options,
            selected: selected_indices,
            highlighted: 0,
        });
    }

    /// 显示 Skill 的 CLI Tool 选择对话框
    fn show_skill_cli_tool_dialog(&mut self, skill: Skill) {
        let cli_tools = AppType::all();
        let mut options = Vec::new();
        let mut selected_indices = Vec::new();

        for (idx, cli_tool) in cli_tools.iter().enumerate() {
            options.push(cli_tool.display_name().to_string());
            if skill.is_enabled_for(cli_tool) {
                selected_indices.push(idx);
            }
        }

        self.dialog = Some(DialogKind::MultiSelect {
            title: format!("Enable Skill: {}", skill.name),
            message: "Select CLI Tools to enable this skill for:".to_string(),
            options,
            selected: selected_indices,
            highlighted: 0,
        });
    }

    pub fn show_notification(&mut self, message: String, level: NotificationLevel) {
        self.notification = Some(Notification {
            message,
            level,
            timestamp: Instant::now(),
        });
    }

    /// 打开 Settings 编辑对话框
    fn open_settings_edit_dialog(&mut self) {
        // 映射：索引 -> (标题, 字段)
        // 分类标题索引: 0, 3, 9, 13, 17, 19, 24
        let (title, fields) = match self.settings_selected {
            // === Logging ===
            1 => ("Log Level", vec![{
                let mut f = InputField::new("Level", "info");
                f.set_value(self.config.log.level.clone());
                f
            }]),
            2 => ("Log File", vec![{
                let mut f = InputField::new("Path", "~/.mrouter/logs/mrouter.log");
                f.set_value(self.config.log.file.clone().unwrap_or_default());
                f
            }]),
            3 => ("Log Max Size (MB)", vec![{
                let mut f = InputField::new("Size", "10");
                f.set_value(self.config.log.max_size_mb.to_string());
                f
            }]),
            4 => ("Log Max Backups", vec![{
                let mut f = InputField::new("Count", "5");
                f.set_value(self.config.log.max_backups.to_string());
                f
            }]),

            // === Database ===
            6 => ("Database Path", vec![{
                let mut f = InputField::new("Path", "~/.mrouter/db/mrouter.db");
                f.set_value(self.config.database.path.clone());
                f
            }]),
            7 => {
                // WAL Mode toggle
                self.config.database.wal_mode = !self.config.database.wal_mode;
                if let Err(e) = self.config.save() {
                    self.show_notification(format!("Failed to save: {}", e), NotificationLevel::Error);
                } else {
                    let status = if self.config.database.wal_mode { "Enabled" } else { "Disabled" };
                    self.show_notification(format!("WAL Mode: {}", status), NotificationLevel::Success);
                }
                return;
            }
            8 => ("Max Request Logs", vec![{
                let mut f = InputField::new("Count", "1000000");
                f.set_value(self.config.database.max_request_logs.to_string());
                f
            }]),
            9 => ("Archive Directory", vec![{
                let mut f = InputField::new("Path", "~/.mrouter/archives");
                f.set_value(self.config.database.archive_dir.clone());
                f
            }]),
            10 => {
                // Auto Cleanup toggle
                self.config.database.auto_cleanup = !self.config.database.auto_cleanup;
                if let Err(e) = self.config.save() {
                    self.show_notification(format!("Failed to save: {}", e), NotificationLevel::Error);
                } else {
                    let status = if self.config.database.auto_cleanup { "Enabled" } else { "Disabled" };
                    self.show_notification(format!("Auto Cleanup: {}", status), NotificationLevel::Success);
                }
                return;
            }

            // === Proxy ===
            12 => ("Proxy Port", vec![{
                let mut f = InputField::new("Port", "4444");
                f.set_value(self.config.proxy.port.to_string());
                f
            }]),
            13 => ("Proxy Bind", vec![{
                let mut f = InputField::new("Address", "127.0.0.1");
                f.set_value(self.config.proxy.bind.clone());
                f
            }]),
            14 => ("Request Timeout", vec![{
                let mut f = InputField::new("Seconds", "30");
                f.set_value(self.config.proxy.timeout_secs.to_string());
                f
            }]),

            // === Streaming Timeout ===
            16 => ("First Byte Timeout", vec![{
                let mut f = InputField::new("Seconds", "10");
                f.set_value(self.config.proxy.streaming_timeout.first_byte_secs.to_string());
                f
            }]),
            17 => ("Idle Timeout", vec![{
                let mut f = InputField::new("Seconds", "30");
                f.set_value(self.config.proxy.streaming_timeout.idle_secs.to_string());
                f
            }]),
            18 => ("Total Timeout", vec![{
                let mut f = InputField::new("Seconds", "300");
                f.set_value(self.config.proxy.streaming_timeout.total_secs.to_string());
                f
            }]),

            // === Health Check ===
            20 => ("Health Check Interval", vec![{
                let mut f = InputField::new("Seconds", "300");
                f.set_value(self.config.health_check.interval_secs.to_string());
                f
            }]),

            // === Circuit Breaker ===
            22 => ("CB Failure Threshold", vec![{
                let mut f = InputField::new("Count", "5");
                f.set_value(self.config.circuit_breaker.failure_threshold.to_string());
                f
            }]),
            23 => ("CB Success Threshold", vec![{
                let mut f = InputField::new("Count", "2");
                f.set_value(self.config.circuit_breaker.success_threshold.to_string());
                f
            }]),
            24 => ("CB Timeout", vec![{
                let mut f = InputField::new("Seconds", "60");
                f.set_value(self.config.circuit_breaker.timeout_secs.to_string());
                f
            }]),
            25 => ("CB Half-Open Timeout", vec![{
                let mut f = InputField::new("Seconds", "30");
                f.set_value(self.config.circuit_breaker.half_open_timeout_secs.to_string());
                f
            }]),

            // === Model Fallback ===
            27 => {
                // Model Fallback toggle
                self.config.model_fallback.enabled = !self.config.model_fallback.enabled;
                if let Err(e) = self.config.save() {
                    self.show_notification(format!("Failed to save: {}", e), NotificationLevel::Error);
                } else {
                    let status = if self.config.model_fallback.enabled { "Enabled" } else { "Disabled" };
                    self.show_notification(format!("Model Fallback: {}", status), NotificationLevel::Success);
                }
                return;
            }

            _ => return,  // 分类标题或无效索引
        };

        self.dialog = Some(DialogKind::Input {
            title: format!("Edit: {}", title),
            fields,
            focused_field: 0,
        });
        self.input_mode = InputMode::Editing;
    }

    /// 保存 Settings 编辑结果
    fn save_settings_edit(&mut self, value: &str) -> Result<()> {
        match self.settings_selected {
            // === Logging ===
            1 => {
                let valid = ["trace", "debug", "info", "warn", "error"];
                if valid.contains(&value) {
                    self.config.log.level = value.to_string();
                } else {
                    self.show_notification(
                        format!("Invalid log level. Use: {}", valid.join(", ")),
                        NotificationLevel::Error,
                    );
                    return Ok(());
                }
            }
            2 => {
                self.config.log.file = if value.is_empty() { None } else { Some(value.to_string()) };
            }
            3 => {
                if let Ok(size) = value.parse::<u64>() {
                    if size >= 1 && size <= 1000 {
                        self.config.log.max_size_mb = size;
                    } else {
                        self.show_notification("Max size must be 1-1000 MB".to_string(), NotificationLevel::Error);
                        return Ok(());
                    }
                } else {
                    self.show_notification("Invalid size value".to_string(), NotificationLevel::Error);
                    return Ok(());
                }
            }
            4 => {
                if let Ok(count) = value.parse::<usize>() {
                    if count >= 1 && count <= 100 {
                        self.config.log.max_backups = count;
                    } else {
                        self.show_notification("Max backups must be 1-100".to_string(), NotificationLevel::Error);
                        return Ok(());
                    }
                } else {
                    self.show_notification("Invalid count value".to_string(), NotificationLevel::Error);
                    return Ok(());
                }
            }

            // === Database ===
            6 => {
                self.config.database.path = value.to_string();
            }
            8 => {
                if let Ok(count) = value.parse::<i64>() {
                    if count >= 1000 && count <= 100_000_000 {
                        self.config.database.max_request_logs = count;
                    } else {
                        self.show_notification("Max logs must be 1000-100000000".to_string(), NotificationLevel::Error);
                        return Ok(());
                    }
                } else {
                    self.show_notification("Invalid count value".to_string(), NotificationLevel::Error);
                    return Ok(());
                }
            }
            9 => {
                self.config.database.archive_dir = value.to_string();
            }

            // === Proxy ===
            12 => {
                if let Ok(port) = value.parse::<u16>() {
                    self.config.proxy.port = port;
                    self.proxy_port = port;
                } else {
                    self.show_notification("Invalid port number".to_string(), NotificationLevel::Error);
                    return Ok(());
                }
            }
            13 => {
                self.config.proxy.bind = value.to_string();
                self.proxy_bind = value.to_string();
            }
            14 => {
                if let Ok(secs) = value.parse::<u64>() {
                    self.config.proxy.timeout_secs = secs;
                } else {
                    self.show_notification("Invalid timeout value".to_string(), NotificationLevel::Error);
                    return Ok(());
                }
            }

            // === Streaming Timeout ===
            16 => {
                if let Ok(secs) = value.parse::<u64>() {
                    if secs >= 1 && secs <= 300 {
                        self.config.proxy.streaming_timeout.first_byte_secs = secs;
                    } else {
                        self.show_notification("First byte timeout must be 1-300 seconds".to_string(), NotificationLevel::Error);
                        return Ok(());
                    }
                } else {
                    self.show_notification("Invalid timeout value".to_string(), NotificationLevel::Error);
                    return Ok(());
                }
            }
            17 => {
                if let Ok(secs) = value.parse::<u64>() {
                    if secs >= 1 && secs <= 600 {
                        self.config.proxy.streaming_timeout.idle_secs = secs;
                    } else {
                        self.show_notification("Idle timeout must be 1-600 seconds".to_string(), NotificationLevel::Error);
                        return Ok(());
                    }
                } else {
                    self.show_notification("Invalid timeout value".to_string(), NotificationLevel::Error);
                    return Ok(());
                }
            }
            18 => {
                if let Ok(secs) = value.parse::<u64>() {
                    if secs >= 10 && secs <= 3600 {
                        self.config.proxy.streaming_timeout.total_secs = secs;
                    } else {
                        self.show_notification("Total timeout must be 10-3600 seconds".to_string(), NotificationLevel::Error);
                        return Ok(());
                    }
                } else {
                    self.show_notification("Invalid timeout value".to_string(), NotificationLevel::Error);
                    return Ok(());
                }
            }

            // === Health Check ===
            20 => {
                if let Ok(secs) = value.parse::<u64>() {
                    self.config.health_check.interval_secs = secs;
                } else {
                    self.show_notification("Invalid interval value".to_string(), NotificationLevel::Error);
                    return Ok(());
                }
            }

            // === Circuit Breaker ===
            22 => {
                if let Ok(count) = value.parse::<u32>() {
                    if count > 0 && count <= 100 {
                        self.config.circuit_breaker.failure_threshold = count;
                    } else {
                        self.show_notification("Failure threshold must be 1-100".to_string(), NotificationLevel::Error);
                        return Ok(());
                    }
                } else {
                    self.show_notification("Invalid failure threshold".to_string(), NotificationLevel::Error);
                    return Ok(());
                }
            }
            23 => {
                if let Ok(count) = value.parse::<u32>() {
                    if count > 0 && count <= 50 {
                        self.config.circuit_breaker.success_threshold = count;
                    } else {
                        self.show_notification("Success threshold must be 1-50".to_string(), NotificationLevel::Error);
                        return Ok(());
                    }
                } else {
                    self.show_notification("Invalid success threshold".to_string(), NotificationLevel::Error);
                    return Ok(());
                }
            }
            24 => {
                if let Ok(secs) = value.parse::<u64>() {
                    if secs >= 10 && secs <= 3600 {
                        self.config.circuit_breaker.timeout_secs = secs;
                    } else {
                        self.show_notification("Timeout must be 10-3600 seconds".to_string(), NotificationLevel::Error);
                        return Ok(());
                    }
                } else {
                    self.show_notification("Invalid timeout value".to_string(), NotificationLevel::Error);
                    return Ok(());
                }
            }
            25 => {
                if let Ok(secs) = value.parse::<u64>() {
                    if secs >= 5 && secs <= 600 {
                        self.config.circuit_breaker.half_open_timeout_secs = secs;
                    } else {
                        self.show_notification("Half-open timeout must be 5-600 seconds".to_string(), NotificationLevel::Error);
                        return Ok(());
                    }
                } else {
                    self.show_notification("Invalid timeout value".to_string(), NotificationLevel::Error);
                    return Ok(());
                }
            }

            _ => {}
        }

        // 保存到文件
        self.config.save()?;
        self.show_notification("Settings saved".to_string(), NotificationLevel::Success);
        Ok(())
    }
    
    pub fn clear_old_notifications(&mut self) {
        if let Some(notif) = &self.notification {
            if notif.timestamp.elapsed().as_secs() > 3 {
                self.notification = None;
            }
        }
    }

    /// 输入字符到当前对话框字段
    pub fn handle_input_char(&mut self, c: char) {
        if let Some(DialogKind::Input { fields, focused_field, .. }) = &mut self.dialog {
            if let Some(field) = fields.get_mut(*focused_field) {
                if field.is_select() {
                    field.select_filter_push(c);
                } else if field.is_checkbox() {
                    // 对于 checkbox，空格键切换状态
                    if c == ' ' {
                        if let FieldKind::Checkbox { checked } = &mut field.kind {
                            *checked = !*checked;
                            field.value = if *checked { "true" } else { "false" }.to_string();
                        }
                    }
                } else {
                    // 在光标位置插入字符
                    let byte_pos = field.cursor_byte_pos();
                    field.value.insert(byte_pos, c);
                    field.cursor_pos += 1;
                    // 用户开始输入，重置手动清空标记
                    field.manually_cleared = false;
                }
            }
        }
    }

    /// 粘贴文本到当前对话框字段（支持中文 IME）
    pub fn handle_input_paste(&mut self, text: &str) {
        if let Some(DialogKind::Input { fields, focused_field, .. }) = &mut self.dialog {
            if let Some(field) = fields.get_mut(*focused_field) {
                if !field.is_select() {
                    // 在光标位置插入文本
                    let byte_pos = field.cursor_byte_pos();
                    field.value.insert_str(byte_pos, text);
                    field.cursor_pos += text.chars().count();
                }
            }
        }
    }

    /// 从剪贴板粘贴到当前对话框字段（Ctrl+V fallback）
    pub fn handle_input_paste_clipboard(&mut self) {
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => {
                match clipboard.get_text() {
                    Ok(text) => {
                        if !text.is_empty() {
                            self.handle_input_paste(&text);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to read clipboard: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to create clipboard for paste: {}", e);
            }
        }
    }

    /// 删除当前对话框字段的最后一个字符
    pub fn handle_input_backspace(&mut self) {
        if let Some(DialogKind::Input { fields, focused_field, .. }) = &mut self.dialog {
            if let Some(field) = fields.get_mut(*focused_field) {
                if field.is_select() {
                    field.select_filter_pop();
                } else if field.cursor_pos > 0 {
                    // 删除光标前的字符
                    field.cursor_pos -= 1;
                    let byte_pos = field.cursor_byte_pos();
                    field.value.remove(byte_pos);
                }
            }
        }
    }

    /// 选择框:下一个选项
    pub fn handle_input_select_next(&mut self) {
        if let Some(DialogKind::Input { fields, focused_field, title, .. }) = &mut self.dialog {
            // 先获取需要的信息
            let should_autofill = (title.contains("Add Provider") || title.contains("Edit Provider"))
                && *focused_field == 0
                && fields.len() > 6;

            if let Some(field) = fields.get_mut(*focused_field) {
                field.select_next();
            }

            // 如果需要自动填充，获取 provider_type 的值
            if should_autofill {
                if let Some(field) = fields.get(0) {
                    // 获取当前高亮的选项值,而不是 field.value
                    if let Some(provider_type_str) = field.get_highlighted_option() {
                        if let Ok(provider_type) = provider_type_str.parse::<crate::models::ProviderType>() {
                            tracing::debug!("Provider type changed to: {:?}", provider_type);

                            // 1. 自动填充 base_url（字段索引 3）
                            let default_base_url = provider_type.default_base_url();
                            if !default_base_url.is_empty() {
                                if let Some(base_url_field) = fields.get_mut(3) {
                                    tracing::debug!("Current base_url: {}", base_url_field.value);
                                    tracing::debug!("Default base_url: {}", default_base_url);

                                    // 如果当前值是默认 URL（空、包含 auto-fill、或是其他 provider 的默认 URL），则更新
                                    let is_default = crate::models::ProviderType::is_default_base_url(&base_url_field.value);
                                    tracing::debug!("Is default URL: {}", is_default);

                                    if is_default {
                                        base_url_field.value = default_base_url.to_string();
                                        tracing::debug!("Updated base_url to: {}", base_url_field.value);
                                    }
                                }
                            }

                            // 2. 自动填充 model 和 supported_models
                            let models_field_cleared = fields.get(6)
                                .map(|f| f.manually_cleared)
                                .unwrap_or(false);

                            if !models_field_cleared {
                                let models = provider_type.default_supported_models();
                                if !models.is_empty() {
                                    // Model 字段：从 supported_models 取第一个（仅当当前值是空或某个 provider 的默认首选）
                                    if let Some(model_field) = fields.get_mut(4) {
                                        let is_default_model = model_field.value.is_empty()
                                            || crate::models::ProviderType::all().iter().any(|pt| {
                                                let dm = pt.default_supported_models();
                                                !dm.is_empty() && dm[0] == model_field.value
                                            });
                                        if is_default_model {
                                            model_field.set_value(models[0].clone());
                                        }
                                    }
                                    if let Some(models_field) = fields.get_mut(6) {
                                        models_field.value = models.join(", ");
                                        models_field.placeholder = "(auto-filled, press Ctrl+F to fetch latest)".to_string();
                                        models_field.manually_cleared = false;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// 选择框：上一个选项
    pub fn handle_input_select_prev(&mut self) {
        if let Some(DialogKind::Input { fields, focused_field, title, .. }) = &mut self.dialog {
            // 先获取需要的信息
            let should_autofill = (title.contains("Add Provider") || title.contains("Edit Provider"))
                && *focused_field == 0
                && fields.len() > 6;

            if let Some(field) = fields.get_mut(*focused_field) {
                field.select_prev();
            }

            // 如果需要自动填充，获取 provider_type 的值
            if should_autofill {
                if let Some(field) = fields.get(0) {
                    // 获取当前高亮的选项值,而不是 field.value
                    if let Some(provider_type_str) = field.get_highlighted_option() {
                        if let Ok(provider_type) = provider_type_str.parse::<crate::models::ProviderType>() {
                            // 1. 自动填充 base_url（字段索引 3）
                            let default_base_url = provider_type.default_base_url();
                            if !default_base_url.is_empty() {
                                if let Some(base_url_field) = fields.get_mut(3) {
                                    // 如果当前值是默认 URL（空、包含 auto-fill、或是其他 provider 的默认 URL），则更新
                                    if crate::models::ProviderType::is_default_base_url(&base_url_field.value) {
                                        base_url_field.value = default_base_url.to_string();
                                    }
                                }
                            }

                            // 2. 自动填充 model 和 supported_models
                            let models_field_cleared = fields.get(6)
                                .map(|f| f.manually_cleared)
                                .unwrap_or(false);

                            if !models_field_cleared {
                                let models = provider_type.default_supported_models();
                                if !models.is_empty() {
                                    if let Some(model_field) = fields.get_mut(4) {
                                        let is_default_model = model_field.value.is_empty()
                                            || crate::models::ProviderType::all().iter().any(|pt| {
                                                let dm = pt.default_supported_models();
                                                !dm.is_empty() && dm[0] == model_field.value
                                            });
                                        if is_default_model {
                                            model_field.set_value(models[0].clone());
                                        }
                                    }
                                    if let Some(models_field) = fields.get_mut(6) {
                                        models_field.value = models.join(", ");
                                        models_field.placeholder = "(auto-filled, press Ctrl+F to fetch latest)".to_string();
                                        models_field.manually_cleared = false;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// 切换到下一个输入字段
    pub fn handle_input_next_field(&mut self) {
        if let Some(DialogKind::Input { fields, focused_field, .. }) = &mut self.dialog {
            // 重置当前字段的滚动偏移量和光标位置
            if let Some(field) = fields.get_mut(*focused_field) {
                field.reset_scroll();
                field.cursor_pos = field.value.chars().count(); // 移动到末尾
            }
            *focused_field = (*focused_field + 1) % fields.len();
        }
    }

    /// 切换到上一个输入字段
    pub fn handle_input_prev_field(&mut self) {
        if let Some(DialogKind::Input { fields, focused_field, .. }) = &mut self.dialog {
            // 重置当前字段的滚动偏移量和光标位置
            if let Some(field) = fields.get_mut(*focused_field) {
                field.reset_scroll();
                field.cursor_pos = field.value.chars().count(); // 移动到末尾
            }
            if *focused_field == 0 {
                *focused_field = fields.len() - 1;
            } else {
                *focused_field -= 1;
            }
        }
    }

    /// 鼠标点击切换到指定字段
    pub fn handle_mouse_click_field(&mut self, field_index: usize) {
        if let Some(DialogKind::Input { fields, focused_field, .. }) = &mut self.dialog {
            if field_index < fields.len() {
                // 重置当前字段的滚动偏移量和光标位置
                if let Some(field) = fields.get_mut(*focused_field) {
                    field.reset_scroll();
                    field.cursor_pos = field.value.chars().count(); // 移动到末尾
                }
                *focused_field = field_index;
            }
        }
    }

    /// Ctrl+U 清空当前字段
    pub fn handle_input_clear(&mut self) {
        if let Some(DialogKind::Input { fields, focused_field, .. }) = &mut self.dialog {
            if let Some(field) = fields.get_mut(*focused_field) {
                if field.is_select() {
                    // 清空过滤文本
                    if let crate::tui::widgets::dialog::FieldKind::Select { filter, options, filtered, highlight, .. } = &mut field.kind {
                        filter.clear();
                        *filtered = (0..options.len()).collect();
                        *highlight = 0;
                    }
                } else {
                    field.value.clear();
                    field.cursor_pos = 0;
                    // 标记为手动清空，阻止自动填充
                    field.manually_cleared = true;
                }
            }
        }
    }

    /// Ctrl+R 切换密码字段明文/密文显示
    pub fn handle_input_toggle_password(&mut self) {
        if let Some(DialogKind::Input { fields, focused_field, .. }) = &mut self.dialog {
            if let Some(field) = fields.get_mut(*focused_field) {
                field.toggle_password_visibility();
            }
        }
    }

    /// Ctrl+K 复制当前字段的值到剪贴板
    pub fn handle_input_copy(&mut self) {
        if let Some(DialogKind::Input { fields, focused_field, .. }) = &self.dialog {
            if let Some(field) = fields.get(*focused_field) {
                if field.value.is_empty() {
                    self.show_notification(
                        "Nothing to copy (field is empty)".to_string(),
                        NotificationLevel::Warning,
                    );
                    return;
                }

                match arboard::Clipboard::new() {
                    Ok(mut clipboard) => {
                        if let Err(e) = clipboard.set_text(field.value.clone()) {
                            tracing::error!("Failed to set clipboard text: {}", e);
                            self.show_notification(
                                format!("Failed to copy: {}", e),
                                NotificationLevel::Error,
                            );
                        } else {
                            self.show_notification(
                                format!("Copied: {}", field.label),
                                NotificationLevel::Success,
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to create clipboard: {}", e);
                        self.show_notification(
                            format!("Clipboard error: {}", e),
                            NotificationLevel::Error,
                        );
                    }
                }
            }
        }
    }

    /// 左箭头：向左滚动文本字段
    pub fn handle_input_scroll_left(&mut self) {
        if let Some(DialogKind::Input { fields, focused_field, .. }) = &mut self.dialog {
            if let Some(field) = fields.get_mut(*focused_field) {
                if !field.is_select() && field.cursor_pos > 0 {
                    // 向左移动光标
                    field.cursor_pos -= 1;
                }
            }
        }
    }

    /// 右箭头：向右移动光标
    pub fn handle_input_scroll_right(&mut self) {
        if let Some(DialogKind::Input { fields, focused_field, .. }) = &mut self.dialog {
            if let Some(field) = fields.get_mut(*focused_field) {
                if !field.is_select() && field.cursor_pos < field.value.chars().count() {
                    // 向右移动光标
                    field.cursor_pos += 1;
                }
            }
        }
    }

    /// Space 键：打开模型列表浏览窗口
    pub fn handle_open_model_viewer(&mut self) {
        if let Some(dialog) = self.dialog.take() {
            if let DialogKind::Input { fields, focused_field, .. } = &dialog {
                // 只在 Supported Models 字段时打开
                if let Some(field) = fields.get(*focused_field) {
                    if field.label.contains("Supported Models") && !field.value.is_empty() {
                        // 解析模型列表（逗号分隔）
                        let models: Vec<String> = field.value
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();

                        if !models.is_empty() {
                            // 保存当前的 Input 对话框状态
                            self.previous_dialog = Some(dialog);
                            // 打开模型浏览窗口
                            self.dialog = Some(DialogKind::ModelListViewer {
                                title: "Supported Models".to_string(),
                                models,
                                scroll_offset: 0,
                            });
                            return;
                        }
                    }
                }
            }
            // 如果不满足条件，恢复原对话框
            self.dialog = Some(dialog);
        }
    }

    /// 模型浏览窗口滚动
    pub fn handle_model_viewer_scroll(&mut self, direction: crate::tui::event::Direction) {
        if let Some(DialogKind::ModelListViewer { scroll_offset, models, .. }) = &mut self.dialog {
            match direction {
                crate::tui::event::Direction::Up => {
                    *scroll_offset = scroll_offset.saturating_sub(1);
                }
                crate::tui::event::Direction::Down => {
                    *scroll_offset = (*scroll_offset + 1).min(models.len().saturating_sub(1));
                }
                _ => {}
            }
        }
    }

    /// 在 Provider 详情页查看支持的模型列表
    pub fn handle_view_supported_models(&mut self) {
        if self.current_tab == Tab::Providers {
            if let Some(provider) = self.providers.get(self.selected_provider) {
                if let Some(models) = &provider.supported_models {
                    if !models.is_empty() {
                        self.dialog = Some(DialogKind::ModelListViewer {
                            title: format!("{} - Supported Models", provider.name),
                            models: models.clone(),
                            scroll_offset: 0,
                        });
                    }
                }
            }
        }
    }

    /// 按 p 键配置 Provider 定价
    pub fn handle_configure_pricing(&mut self) {
        if self.current_tab == Tab::Providers {
            if let Some(provider) = self.providers.get(self.selected_provider) {
                let pricing = provider.pricing();

                let mut fields = vec![
                    InputField::new("Input Price ($/M)", "Price per million input tokens"),
                    InputField::new("Output Price ($/M)", "Price per million output tokens"),
                    InputField::new("Cache Write Price ($/M)", "Price per million cache write tokens"),
                    InputField::new("Cache Read Price ($/M)", "Price per million cache read tokens"),
                ];

                // 预填充当前定价
                fields[0].set_value(pricing.input_price_per_million.to_string());
                fields[1].set_value(pricing.output_price_per_million.to_string());
                fields[2].set_value(pricing.cache_write_price_per_million.to_string());
                fields[3].set_value(pricing.cache_read_price_per_million.to_string());

                self.dialog = Some(DialogKind::Input {
                    title: format!("Configure Pricing: {}", provider.name),
                    fields,
                    focused_field: 0,
                });
                self.input_mode = InputMode::Editing;
            }
        }
    }

    /// 按 m 键配置模型名称映射
    pub fn handle_configure_model_mappings(&mut self) {
        if self.current_tab == Tab::Providers {
            if let Some(provider) = self.providers.get(self.selected_provider) {
                let mappings = provider.model_mappings();

                // 将映射转换为字符串格式：alias1=actual1, alias2=actual2
                let mappings_str = mappings.iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join(", ");

                let mut fields = vec![
                    InputField::new(
                        "Model Mappings",
                        "Format: alias1=actual1, alias2=actual2 (e.g., mco=claude-code-opus, gpt4=gpt-4-turbo)"
                    ),
                ];

                fields[0].set_value(mappings_str);

                self.dialog = Some(DialogKind::Input {
                    title: format!("Configure Model Mappings: {}", provider.name),
                    fields,
                    focused_field: 0,
                });
                self.input_mode = InputMode::Editing;
            }
        }
    }

    pub fn handle_configure_headers(&mut self) {
        if self.current_tab == Tab::Providers {
            if let Some(provider) = self.providers.get(self.selected_provider) {
                let current_auth = provider.config.get("auth_header")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| provider.provider_type.default_auth_header_spec().to_string());

                let current_custom = provider.config.get("custom_headers")
                    .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
                    .unwrap_or_default();

                let mut fields = vec![
                    InputField::new(
                        "Auth Header",
                        "header-name or header-name:prefix (e.g., x-api-key, authorization:Bearer)"
                    ),
                    InputField::new(
                        "Custom Headers (JSON)",
                        r#"{"User-Agent": "claude-cli/2.1.72", "X-Custom": "value"}"#
                    ),
                ];

                fields[0].set_value(current_auth);
                fields[1].set_value(current_custom);

                self.dialog = Some(DialogKind::Input {
                    title: format!("Configure Headers: {}", provider.name),
                    fields,
                    focused_field: 0,
                });
                self.input_mode = InputMode::Editing;
            }
        }
    }

    /// Ctrl+F 手动从 API 获取模型列表
    pub async fn handle_fetch_models(&mut self) -> Result<()> {
        // 先提取所有需要的信息，避免借用冲突
        let (provider_type, api_key, base_url) = if let Some(DialogKind::Input { fields, title, .. }) = &self.dialog {
            // 只在 Provider 对话框中处理
            if !(title.contains("Add Provider") || title.contains("Edit Provider")) {
                return Ok(());
            }

            let pt = fields.get(0)
                .and_then(|f| f.value.parse::<crate::models::ProviderType>().ok());

            let key = fields.get(2)
                .filter(|f| !f.value.is_empty())
                .map(|f| f.value.clone());

            let url = fields.get(3)
                .filter(|f| !f.value.is_empty())
                .map(|f| f.value.clone());

            (pt, key, url)
        } else {
            return Ok(());
        };

        if let Some(pt) = provider_type {
            // 显示加载提示
            self.show_notification("Fetching models from API...".to_string(), NotificationLevel::Info);

            // 从 API 获取模型列表
            let api_key_ref = api_key.as_deref();
            let base_url_ref = base_url.as_deref();

            match crate::services::ModelService::fetch_models(&pt, api_key_ref, base_url_ref).await {
                Ok(models) if !models.is_empty() => {
                    // 更新 supported_models 字段
                    if let Some(DialogKind::Input { fields, .. }) = &mut self.dialog {
                        if let Some(models_field) = fields.get_mut(6) {
                            models_field.value = models.join(", ");
                            models_field.placeholder = format!("(fetched {} models from API)", models.len());
                        }
                    }

                    // 更新缓存
                    let _ = crate::services::ModelService::update_cache(
                        &pt,
                        api_key_ref,
                        base_url_ref,
                    ).await;

                    self.show_notification(
                        format!("Fetched {} models successfully", models.len()),
                        NotificationLevel::Success,
                    );
                }
                Ok(_) => {
                    self.show_notification(
                        "No models returned from API".to_string(),
                        NotificationLevel::Warning,
                    );
                }
                Err(e) => {
                    self.show_notification(
                        format!("Failed to fetch models: {}", e),
                        NotificationLevel::Error,
                    );
                }
            }
        } else {
            self.show_notification(
                "Please select a Provider Type first".to_string(),
                NotificationLevel::Warning,
            );
        }

        Ok(())
    }

    /// MultiSelect: 切换当前高亮项的选中状态
    pub fn handle_multiselect_toggle(&mut self) {
        if let Some(DialogKind::MultiSelect { selected, highlighted, .. }) = &mut self.dialog {
            if let Some(pos) = selected.iter().position(|&i| i == *highlighted) {
                selected.remove(pos);
            } else {
                selected.push(*highlighted);
                selected.sort_unstable();
            }
        }
    }

    /// MultiSelect: 移动高亮
    pub fn handle_multiselect_navigate(&mut self, up: bool) {
        if let Some(DialogKind::MultiSelect { options, highlighted, .. }) = &mut self.dialog {
            if up {
                if *highlighted > 0 {
                    *highlighted -= 1;
                }
            } else {
                if *highlighted < options.len().saturating_sub(1) {
                    *highlighted += 1;
                }
            }
        }
    }

    /// MultiSelect: 提交选择
    pub async fn handle_multiselect_submit(&mut self) -> Result<()> {
        if let Some(DialogKind::MultiSelect { title: _, options, selected, .. }) = &self.dialog {
            match self.current_tab {
                Tab::Providers => {
                    if let Some(provider) = self.providers.get(self.selected_provider) {
                        let _provider_id = provider.id;

                        // 将选中的索引转换为 CLI Tool 名称
                        let cli_tools: Vec<String> = selected.iter()
                            .filter_map(|&idx| options.get(idx))
                            .filter_map(|name| {
                                // 将显示名称转换为 CLI Tool 字符串
                                AppType::all().iter()
                                    .find(|at| at.display_name() == name)
                                    .map(|at| at.as_str().to_string())
                            })
                            .collect();

                        if cli_tools.is_empty() {
                            // 空选择 → 直接清除同步设置
                            ProviderSwitchService::set_sync_to_cli_tools(&self.db, _provider_id, vec![], None)?;
                            self.show_notification("Proxy-only mode".to_string(), NotificationLevel::Success);
                            self.refresh().await?;
                        } else {
                            // 保存待同步的 CLI Tools，弹出 Confirm 选择同步模式
                            self.pending_sync_cli_tools = Some(cli_tools);
                            let connect_host = if self.proxy_bind == "0.0.0.0" { "127.0.0.1" } else { &self.proxy_bind };
                            self.dialog = Some(DialogKind::Confirm {
                                title: "Sync Mode".to_string(),
                                message: format!(
                                    "[y] Proxy (via mrouter http://{}:{})  [n] Direct (original config)",
                                    connect_host, self.proxy_port
                                ),
                            });
                            return Ok(());
                        }
                    }
                }
                Tab::Mcp => {
                    if let Some(server) = self.mcp_servers.get_mut(self.selected_mcp) {
                        // 重置所有 CLI Tool 的启用状态
                        for cli_tool in AppType::all() {
                            server.set_enabled_for(&cli_tool, false);
                        }

                        // 设置选中的 CLI Tool
                        for &idx in selected {
                            if let Some(name) = options.get(idx) {
                                if let Some(cli_tool) = AppType::all().iter().find(|at| at.display_name() == name) {
                                    server.set_enabled_for(cli_tool, true);
                                }
                            }
                        }

                        let server_name = server.name.clone();
                        McpDao::update(&self.db, server)?;
                        self.show_notification(
                            format!("Updated MCP: {}", server_name),
                            NotificationLevel::Success,
                        );
                    }
                }
                Tab::Stats => {
                    if let Some(skill) = self.skills.get_mut(self.selected_skill) {
                        // 重置所有 CLI Tool 的启用状态
                        for cli_tool in AppType::all() {
                            skill.set_enabled_for(&cli_tool, false);
                        }

                        // 设置选中的 CLI Tool
                        for &idx in selected {
                            if let Some(name) = options.get(idx) {
                                if let Some(cli_tool) = AppType::all().iter().find(|at| at.display_name() == name) {
                                    skill.set_enabled_for(cli_tool, true);
                                }
                            }
                        }

                        let skill_name = skill.name.clone();
                        SkillDao::update(&self.db, skill)?;
                        self.show_notification(
                            format!("Updated skill: {}", skill_name),
                            NotificationLevel::Success,
                        );
                    }
                }
                _ => {}
            }
        }

        self.dialog = None;
        Ok(())
    }

    /// 启动 Proxy
    pub async fn handle_proxy_start(&mut self) -> Result<()> {
        // 检查端口是否被占用
        let port_in_use = self.is_port_in_use(self.proxy_port).await;

        // 检查 daemon 是否在运行
        self.check_proxy_status().await?;

        // 如果 proxy 已经在运行
        if self.proxy_running {
            // 检查是否在正确的端口上运行
            if port_in_use {
                // 端口被占用，可能是 daemon 在正确端口运行
                self.show_notification(
                    format!("Proxy is already running on port {}", self.proxy_port),
                    NotificationLevel::Success,
                );
                return Ok(());
            } else {
                // Daemon 在运行但不在配置的端口上，需要重启
                self.show_notification(
                    "Config changed. Restarting proxy...".to_string(),
                    NotificationLevel::Info,
                );

                // 先停止
                let exe_path = std::env::current_exe()
                    .unwrap_or_else(|_| std::path::PathBuf::from("mrouter"));

                let _ = tokio::process::Command::new(&exe_path)
                    .arg("daemon")
                    .arg("stop")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .output()
                    .await;

                // 等待停止完成
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        } else if port_in_use {
            // Daemon 未运行但端口被占用
            self.show_notification(
                format!("Port {} is already in use. Change port or stop the conflicting process.", self.proxy_port),
                NotificationLevel::Error,
            );
            return Ok(());
        }

        // 获取当前可执行文件路径
        let exe_path = std::env::current_exe()
            .unwrap_or_else(|_| std::path::PathBuf::from("mrouter"));

        // 使用 daemon 启动 proxy（后台运行）
        // 重定向 stdout 和 stderr 到 null，避免破坏 TUI 界面
        let mut cmd = tokio::process::Command::new(&exe_path);
        cmd.arg("daemon")
            .arg("start")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        // Windows: 让 daemon 完全脱离 TUI 进程
        // - CREATE_NO_WINDOW: 不创建控制台窗口
        // - DETACHED_PROCESS: 脱离父进程的控制台
        // - CREATE_BREAKAWAY_FROM_JOB: 脱离父进程的 Job Object
        //   (Windows Terminal 等会将子进程加入 Job Object，
        //    父进程退出时 Job 中所有进程都会被终止)
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            const DETACHED_PROCESS: u32 = 0x00000008;
            const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x01000000;
            cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS | CREATE_BREAKAWAY_FROM_JOB);
        }

        let result = cmd.spawn();

        match result {
            Ok(_) => {
                // 轮询检查 daemon 是否启动成功（最多等待 3 秒）
                let mut started = false;
                for _ in 0..10 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                    // 检查端口是否开始监听
                    if self.is_port_in_use(self.proxy_port).await {
                        self.proxy_running = true;
                        started = true;
                        break;
                    }
                }

                if started {
                    self.show_notification(
                        format!("Proxy started on port {}", self.proxy_port),
                        NotificationLevel::Success,
                    );
                } else {
                    self.show_notification(
                        "Proxy start timeout. Check logs: mrouter daemon logs".to_string(),
                        NotificationLevel::Warning,
                    );
                }
            }
            Err(e) => {
                self.show_notification(
                    format!("Failed to start proxy: {}", e),
                    NotificationLevel::Error,
                );
            }
        }

        Ok(())
    }

    /// 停止 Proxy
    pub async fn handle_proxy_stop(&mut self) -> Result<()> {
        if !self.proxy_running {
            self.show_notification(
                "Proxy is not running".to_string(),
                NotificationLevel::Warning,
            );
            return Ok(());
        }

        // 获取当前可执行文件路径
        let exe_path = std::env::current_exe()
            .unwrap_or_else(|_| std::path::PathBuf::from("mrouter"));

        // 使用 daemon 停止 proxy
        // 重定向 stdout 和 stderr 到 null，避免破坏 TUI 界面
        let result = tokio::process::Command::new(&exe_path)
            .arg("daemon")
            .arg("stop")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .await;

        match result {
            Ok(_) => {
                // 等待一小段时间让 daemon 停止
                tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

                // 检查是否真的停止了
                self.check_proxy_status().await?;

                if !self.proxy_running {
                    self.show_notification(
                        "Proxy stopped".to_string(),
                        NotificationLevel::Success,
                    );
                } else {
                    self.show_notification(
                        "Proxy stop command sent, but status check shows still running".to_string(),
                        NotificationLevel::Warning,
                    );
                }
            }
            Err(e) => {
                self.show_notification(
                    format!("Failed to stop proxy: {}", e),
                    NotificationLevel::Error,
                );
            }
        }

        Ok(())
    }

    /// 编辑 Proxy 端口
    pub async fn handle_proxy_edit_port(&mut self) -> Result<()> {
        let mut bind_field = InputField::new("Bind Address", "127.0.0.1");
        bind_field.set_value(self.proxy_bind.clone());

        let mut port_field = InputField::new("Port", "4444");
        port_field.set_value(self.proxy_port.to_string());

        self.dialog = Some(DialogKind::Input {
            title: "Edit Proxy Config".to_string(),
            fields: vec![bind_field, port_field],
            focused_field: 0,
        });
        self.input_mode = InputMode::Editing;
        Ok(())
    }

    /// 重置当前选中 Provider 的 Circuit Breaker
    pub async fn handle_reset_circuit_breaker(&mut self) -> Result<()> {
        if !self.proxy_running {
            self.show_notification(
                "Proxy is not running. Start proxy first.".to_string(),
                NotificationLevel::Warning,
            );
            return Ok(());
        }

        if let Some(provider) = self.providers.get(self.selected_provider) {
            let provider_name = provider.name.clone();
            self.dialog = Some(DialogKind::Confirm {
                title: "Reset Circuit Breaker".to_string(),
                message: format!("Reset circuit breaker for '{}'?\nThis will restart the proxy.", provider_name),
            });
        }
        Ok(())
    }

    /// 切换统计时间范围
    pub fn handle_toggle_stats_time_range(&mut self) -> Result<()> {
        self.stats_time_range = match self.stats_time_range {
            StatsTimeRange::Today => StatsTimeRange::Week,
            StatsTimeRange::Week => StatsTimeRange::Month,
            StatsTimeRange::Month => StatsTimeRange::All,
            StatsTimeRange::All => StatsTimeRange::Today,
        };
        self.refresh_stats()?;
        Ok(())
    }

    /// 检查 Proxy 状态
    pub async fn check_proxy_status(&mut self) -> Result<()> {
        // 直接检查 PID 文件和进程是否存在
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
        let pid_file = home.join(".mrouter").join("daemon.pid");

        if !pid_file.exists() {
            self.proxy_running = false;
            return Ok(());
        }

        // 读取 PID
        let pid_str = fs::read_to_string(&pid_file)?;
        let pid: u32 = pid_str.trim().parse()?;

        // 使用 kill -0 检查进程是否存在
        #[cfg(unix)]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .output()
            {
                self.proxy_running = output.status.success();
            } else {
                self.proxy_running = false;
            }
        }

        #[cfg(not(unix))]
        {
            // Windows: 使用 tasklist 检查进程是否存在
            let output = std::process::Command::new("tasklist")
                .arg("/FI")
                .arg(format!("PID eq {}", pid))
                .arg("/NH")
                .output();

            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    self.proxy_running = stdout.contains(&pid.to_string());
                }
                _ => {
                    self.proxy_running = false;
                }
            }

            // 如果进程不存在，清理 stale PID 文件
            if !self.proxy_running {
                fs::remove_file(&pid_file).ok();
            }
        }

        Ok(())
    }

    /// 检查端口是否被占用
    async fn is_port_in_use(&self, port: u16) -> bool {
        use tokio::net::TcpStream;
        use std::time::Duration;

        // 确定连接检测地址：
        // - 0.0.0.0 / :: 是通配地址，无法直接连接，改用 127.0.0.1
        // - 其他地址（127.0.0.1, 192.168.x.x 等）直接连接
        let check_addr = match self.proxy_bind.as_str() {
            "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
            addr => addr.to_string(),
        };
        let addr = format!("{}:{}", check_addr, port);
        // 设置连接超时，避免在端口未监听时长时间阻塞
        match tokio::time::timeout(Duration::from_millis(500), TcpStream::connect(&addr)).await {
            Ok(Ok(_)) => true,
            _ => false,
        }
    }
}
