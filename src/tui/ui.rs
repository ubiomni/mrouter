// UI 渲染

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Frame,
};
use crate::app::{App, Tab};
use crate::models::AppType;
use crate::tui::theme;
use crate::tui::widgets::dialog::{self, DialogKind};
use crate::tui::tabs;

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // 标题栏
            Constraint::Length(3),  // 标签页
            Constraint::Min(0),     // 主内容
            Constraint::Length(3),  // 状态栏
        ])
        .split(f.area());

    // 渲染标题栏
    render_header(f, chunks[0], app);

    // 渲染标签页
    render_tabs(f, chunks[1], app);

    // 渲染主内容
    match app.current_tab {
        Tab::Providers => render_providers(f, chunks[2], app),
        // Tab::Mcp => render_mcp(f, chunks[2], app),  // 暂时隐藏
        Tab::Stats => render_stats(f, chunks[2], app),  // 用于显示统计
        Tab::Proxy => render_proxy(f, chunks[2], app),
        Tab::RequestLogs => tabs::request_logs::render(f, app, chunks[2]),
        Tab::Settings => render_settings(f, chunks[2], app),
        _ => {
            // MCP 暂时隐藏,显示提示信息
            let info = Paragraph::new("This feature is temporarily hidden.\n\nPress [1] for Providers, [2] for Proxy, [3] for Logs, [4] for Stats, [5] for Settings.")
                .block(Block::default().borders(Borders::ALL).title(" Info "));
            f.render_widget(info, chunks[2]);
        }
    }

    // 渲染状态栏
    render_status_bar(f, chunks[3], app);

    // 渲染通知
    if let Some(notification) = &app.notification {
        render_notification(f, notification);
    }

    // 渲染对话框（最上层）
    if let Some(dialog_kind) = &app.dialog {
        match dialog_kind {
            DialogKind::Confirm { title, message } => {
                dialog::render_confirm_dialog(f, title, message);
            }
            DialogKind::Input { title, fields, focused_field } => {
                dialog::render_input_dialog(f, title, fields, *focused_field);
            }
            DialogKind::MultiSelect { title, message, options, selected, highlighted } => {
                dialog::render_multiselect_dialog(f, title, message, options, selected, *highlighted);
            }
            DialogKind::ModelListViewer { title, models, scroll_offset } => {
                dialog::render_model_list_viewer(f, title, models, *scroll_offset);
            }
            DialogKind::Help => {
                dialog::render_help_dialog(f);
            }
        }
    }

    // 渲染帮助对话框
    if app.show_help {
        dialog::render_help_dialog(f);
    }
}

fn render_header(f: &mut Frame, area: Rect, _app: &App) {
    let title = " MRouter v0.1.0  │  LLM Gateway Router  │  [?] Help ";
    let header = Paragraph::new(title)
        .style(Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme::BORDER)));
    f.render_widget(header, area);
}

fn render_tabs(f: &mut Frame, area: Rect, app: &App) {
    let titles = vec!["[1] Providers", "[2] Proxy", "[3] Logs", "[4] Stats", "[5] Settings"];
    let selected = match app.current_tab {
        Tab::Providers => 0,
        // Tab::Mcp => 1,  // 隐藏
        Tab::Stats => 3,  // Stats 标签页
        Tab::Proxy => 1,
        Tab::RequestLogs => 2,
        Tab::Settings => 4,
        _ => 0,
    };

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme::BORDER)))
        .select(selected)
        .style(Style::default().fg(theme::TEXT))
        .highlight_style(Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))
        .divider("│");

    f.render_widget(tabs, area);
}

fn render_providers(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);

    // 左侧：Provider 列表
    let items: Vec<ListItem> = app
        .providers
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let icon = if p.is_active { "●" } else { "○" };
            let selected = i == app.selected_provider;

            // 优化颜色方案：选中时使用高对比度
            let (icon_color, type_color, name_color, sync_color) = if selected {
                // 选中状态：使用明亮的颜色
                (
                    if p.is_active { theme::GREEN } else { theme::YELLOW },  // 未激活用黄色更醒目
                    theme::YELLOW,
                    theme::CYAN,  // 名称使用青色，更醒目
                    theme::MAGENTA,
                )
            } else {
                // 未选中状态：使用柔和但清晰的颜色
                (
                    if p.is_active { theme::GREEN } else { theme::MUTED },
                    theme::YELLOW,
                    theme::TEXT,  // 统一使用白色，通过 icon 区分激活状态
                    theme::MUTED,
                )
            };

            // 显示同步状态 - 简化显示
            let sync_info = if p.sync_to_cli_tools.is_empty() {
                "Proxy".to_string()
            } else if p.sync_to_cli_tools.len() == 1 {
                p.sync_to_cli_tools[0].clone()
            } else {
                format!("{}+{}", p.sync_to_cli_tools[0], p.sync_to_cli_tools.len() - 1)
            };

            // 添加 padding 和更好的布局
            let content = Line::from(vec![
                Span::raw("  "),  // 左侧 padding
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::raw("  "),  // icon 和 type 之间的间距
                Span::styled(
                    format!("{:<12}", p.provider_type.display_name()),  // 固定宽度对齐
                    Style::default().fg(type_color)
                ),
                Span::raw(" "),
                Span::styled(
                    &p.name,
                    Style::default()
                        .fg(name_color)
                        .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() })
                ),
                Span::raw("  "),
                Span::styled(
                    format!("[{}]", sync_info),
                    Style::default().fg(sync_color)
                ),
            ]);

            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Providers (All) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER))
                .padding(ratatui::widgets::Padding::new(0, 0, 1, 1))  // 上下 padding
        );

    f.render_widget(list, chunks[0]);

    // 右侧：Provider 详情
    if let Some(provider) = app.providers.get(app.selected_provider) {
        let sync_display = if provider.sync_to_cli_tools.is_empty() {
            "Proxy-only (no config sync)".to_string()
        } else {
            provider.sync_to_cli_tools.iter()
                .map(|t| {
                    t.parse::<crate::models::AppType>()
                        .map(|at| at.display_name())
                        .unwrap_or(t.as_str())
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        let detail_lines = vec![
            Line::from(""),  // 顶部 padding
            Line::from(vec![
                Span::raw("  "),  // 左侧 padding
                Span::styled("Type:      ", Style::default().fg(theme::YELLOW)),
                Span::styled(provider.provider_type.display_name(), Style::default().fg(theme::CYAN)),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Name:      ", Style::default().fg(theme::YELLOW)),
                Span::styled(&provider.name, Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Status:    ", Style::default().fg(theme::YELLOW)),
                Span::styled(
                    if provider.is_active { "● Active" } else { "○ Inactive" },
                    Style::default().fg(if provider.is_active { theme::GREEN } else { theme::YELLOW }),
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Priority:  ", Style::default().fg(theme::YELLOW)),
                Span::styled(provider.priority.to_string(), Style::default().fg(theme::CYAN)),
            ]),
            Line::from(""),  // 分隔空行
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Base URL:  ", Style::default().fg(theme::YELLOW)),
                Span::styled(&provider.base_url, Style::default().fg(theme::CYAN)),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Model:     ", Style::default().fg(theme::YELLOW)),
                Span::styled(provider.model.as_deref().unwrap_or("default"), Style::default().fg(theme::CYAN)),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("API Key:   ", Style::default().fg(theme::YELLOW)),
                Span::styled(mask_api_key(&provider.api_key), Style::default().fg(theme::MUTED)),
            ]),
            Line::from(""),  // 分隔空行
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Supported: ", Style::default().fg(theme::YELLOW)),
                Span::styled(
                    provider.supported_models.as_ref()
                        .map(|models| {
                            if models.len() > 5 {
                                format!("{} models (press v to view)", models.len())
                            } else if !models.is_empty() {
                                models.join(", ")
                            } else {
                                "None".to_string()
                            }
                        })
                        .unwrap_or_else(|| "All models (fallback)".to_string()),
                    Style::default().fg(theme::GREEN)
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Sync To:   ", Style::default().fg(theme::YELLOW)),
                Span::styled(sync_display, Style::default().fg(theme::MAGENTA)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "[Enter] Toggle  [e] Edit  [d] Delete  [a] Add  [s] Sync  [r] Reset CB",
                    Style::default().fg(theme::TEXT),
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "[v] Models  [p] Pricing  [m] Mappings  [o] Auth Header",
                    Style::default().fg(theme::TEXT),
                ),
            ]),
        ];

        let detail = Paragraph::new(detail_lines)
            .block(
                Block::default()
                    .title(" Details ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::BORDER))
            );
        f.render_widget(detail, chunks[1]);
    } else {
        let empty = Paragraph::new("No providers configured.\n\nPress [a] to add a provider.")
            .style(Style::default().fg(theme::MUTED))
            .block(
                Block::default()
                    .title(" Details ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::BORDER))
            );
        f.render_widget(empty, chunks[1]);
    }
}

fn render_mcp(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .mcp_servers
        .iter()
        .enumerate()
        .map(|(i, s)| {
            // 显示所有启用的 CLI Tools
            let enabled_tools: Vec<String> = AppType::all()
                .iter()
                .filter(|at| s.is_enabled_for(at))
                .map(|at| at.as_str().to_string())
                .collect();

            let enabled_info = if enabled_tools.is_empty() {
                "[None]".to_string()
            } else {
                format!("[{}]", enabled_tools.join(","))
            };

            let selected = i == app.selected_mcp;
            let style = if selected {
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let content = Line::from(vec![
                Span::styled(&s.name, style),
                Span::raw(" "),
                Span::styled(format!("({})", s.command), Style::default().fg(theme::YELLOW)),
                Span::raw(" "),
                Span::styled(enabled_info, Style::default().fg(theme::MUTED)),
            ]);

            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" MCP Servers ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER))
        );

    f.render_widget(list, area);
}

fn render_skills(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);

    // 左侧：Skill 列表
    if app.skills.is_empty() {
        let empty = Paragraph::new(
            "No skills installed.\n\nPress [a] to add a skill repository.\n\nSkills are loaded from Git repositories.\nEach repo can contain multiple skills."
        )
            .style(Style::default().fg(theme::MUTED))
            .block(
                Block::default()
                    .title(" Skills ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::BORDER))
            );
        f.render_widget(empty, chunks[0]);
    } else {
        let items: Vec<ListItem> = app
            .skills
            .iter()
            .enumerate()
            .map(|(i, s)| {
                // 显示所有启用的 CLI Tools
                let enabled_tools: Vec<String> = AppType::all()
                    .iter()
                    .filter(|at| s.is_enabled_for(at))
                    .map(|at| at.as_str().to_string())
                    .collect();

                let enabled_info = if enabled_tools.is_empty() {
                    "[None]".to_string()
                } else {
                    format!("[{}]", enabled_tools.join(","))
                };

                let selected = i == app.selected_skill;
                let style = if selected {
                    Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let content = Line::from(vec![
                    Span::styled(&s.name, style),
                    Span::raw(" "),
                Span::styled(enabled_info, Style::default().fg(theme::MUTED)),
                ]);

                ListItem::new(content)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(format!(" Skills [{}/{}] ", app.selected_skill + 1, app.skills.len()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::BORDER))
            );

        f.render_widget(list, chunks[0]);
    }

    // 右侧：Skill 详情 + Repo 信息
    let mut detail_lines: Vec<Line> = Vec::new();

    if let Some(skill) = app.skills.get(app.selected_skill) {
        detail_lines.push(Line::from(vec![
            Span::styled("Name:   ", Style::default().fg(theme::YELLOW)),
            Span::raw(&skill.name),
        ]));

        detail_lines.push(Line::from(vec![
            Span::styled("Path:   ", Style::default().fg(theme::YELLOW)),
            Span::styled(&skill.path, Style::default().fg(theme::MUTED)),
        ]));

        if let Some(desc) = &skill.description {
            detail_lines.push(Line::from(vec![
                Span::styled("Desc:   ", Style::default().fg(theme::YELLOW)),
                Span::raw(desc.as_str()),
            ]));
        }

        detail_lines.push(Line::from(""));

        // 各工具启用状态
        detail_lines.push(Line::from(Span::styled("Enabled for:", Style::default().fg(theme::YELLOW))));
        let tools = [
            ("Claude Code", skill.enabled_claude),
            ("Codex", skill.enabled_codex),
            ("Gemini CLI", skill.enabled_gemini),
            ("OpenCode", skill.enabled_opencode),
        ];
        for (name, enabled) in &tools {
            let icon = if *enabled { "✓" } else { "✗" };
            let color = if *enabled { theme::GREEN } else { theme::MUTED };
            detail_lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(icon, Style::default().fg(color)),
                Span::raw(format!(" {}", name)),
            ]));
        }

        // Repo 信息
        if let Some(repo) = app.skill_repos.iter().find(|r| r.id == skill.repo_id) {
            detail_lines.push(Line::from(""));
            detail_lines.push(Line::from(Span::styled("Repository:", Style::default().fg(theme::YELLOW))));
            detail_lines.push(Line::from(format!("  {} ({})", repo.name, repo.branch)));
            detail_lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(&repo.url, Style::default().fg(theme::MUTED)),
            ]));
            if let Some(synced) = &repo.last_synced {
                detail_lines.push(Line::from(vec![
                    Span::raw("  Synced: "),
                    Span::styled(synced.as_str(), Style::default().fg(theme::MUTED)),
                ]));
            }
        }
    }

    detail_lines.push(Line::from(""));
    detail_lines.push(Line::from(Span::styled(
        "[Enter] Toggle  [e] Edit  [d] Delete  [a] Add Repo",
        Style::default().fg(theme::MUTED),
    )));

    // Repos 摘要
    if !app.skill_repos.is_empty() {
        detail_lines.push(Line::from(""));
        detail_lines.push(Line::from(Span::styled(
            format!("Repositories: {}", app.skill_repos.len()),
            Style::default().fg(theme::YELLOW),
        )));
        for repo in &app.skill_repos {
            let skill_count = app.skills.iter().filter(|s| s.repo_id == repo.id).count();
            detail_lines.push(Line::from(format!("  {} ({} skills)", repo.name, skill_count)));
        }
    }

    let detail = Paragraph::new(detail_lines)
        .block(
            Block::default()
                .title(" Details ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER))
        );
    f.render_widget(detail, chunks[1]);
}

fn render_proxy(f: &mut Frame, area: Rect, app: &App) {
    let status_icon = if app.proxy_running { "●" } else { "○" };
    let status_color = if app.proxy_running { theme::GREEN } else { theme::RED };
    let status_text = if app.proxy_running { "Running" } else { "Stopped" };

    // 构建 URL
    let proxy_url = if app.proxy_bind == "0.0.0.0" {
        format!("http://<your-ip>:{}", app.proxy_port)
    } else {
        format!("http://{}:{}", app.proxy_bind, app.proxy_port)
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Status:   ", Style::default().fg(theme::YELLOW)),
            Span::styled(status_icon, Style::default().fg(status_color)),
            Span::raw(" "),
            Span::styled(status_text, Style::default().fg(status_color)),
        ]),
        Line::from(vec![
            Span::styled("  Bind:     ", Style::default().fg(theme::YELLOW)),
            Span::styled(&app.proxy_bind, Style::default().fg(theme::CYAN)),
        ]),
        Line::from(vec![
            Span::styled("  Port:     ", Style::default().fg(theme::YELLOW)),
            Span::styled(format!("{}", app.proxy_port), Style::default().fg(theme::CYAN)),
        ]),
        Line::from(vec![
            Span::styled("  Requests: ", Style::default().fg(theme::YELLOW)),
            Span::raw(format!("{}", app.proxy_request_count)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  URL:      ", Style::default().fg(theme::YELLOW)),
            Span::styled(proxy_url, Style::default().fg(theme::CYAN)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Controls:",
            Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            if app.proxy_running {
                "    [p] Stop Proxy  [e] Edit Config  [r] Refresh Status"
            } else {
                "    [s] Start Proxy  [e] Edit Config  [r] Refresh Status"
            },
            Style::default().fg(theme::CYAN),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Note: Proxy runs in background. Exit TUI with [q] to keep it running.",
            Style::default().fg(theme::MUTED).add_modifier(Modifier::ITALIC),
        )),
        Line::from(Span::styled(
            "  Tip: Set bind to 0.0.0.0 to allow LAN access, or 192.168.x.x for specific IP.",
            Style::default().fg(theme::MUTED).add_modifier(Modifier::ITALIC),
        )),
    ];

    let text = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Proxy Server ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER))
        );
    f.render_widget(text, area);
}

fn render_stats(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12),  // 总体统计
            Constraint::Min(0),      // 按 Provider 统计
        ])
        .split(area);

    // 总体统计
    if let Some(summary) = &app.stats_summary {
        let time_range_str = match app.stats_time_range {
            crate::app::StatsTimeRange::Today => "Today",
            crate::app::StatsTimeRange::Week => "Last 7 Days",
            crate::app::StatsTimeRange::Month => "Last 30 Days",
            crate::app::StatsTimeRange::All => "All Time",
        };

        let summary_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Time Range:      ", Style::default().fg(theme::YELLOW)),
                Span::styled(time_range_str, Style::default().fg(theme::CYAN)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Total Requests:  ", Style::default().fg(theme::YELLOW)),
                Span::styled(format!("{}", summary.total_requests), Style::default().fg(theme::GREEN)),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Input Tokens:    ", Style::default().fg(theme::YELLOW)),
                Span::styled(format!("{}", summary.total_input_tokens), Style::default().fg(theme::CYAN)),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Output Tokens:   ", Style::default().fg(theme::YELLOW)),
                Span::styled(format!("{}", summary.total_output_tokens), Style::default().fg(theme::MAGENTA)),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Total Tokens:    ", Style::default().fg(theme::YELLOW)),
                Span::styled(format!("{}", summary.total_tokens), Style::default().fg(theme::GREEN)),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Total Cost:      ", Style::default().fg(theme::YELLOW)),
                Span::styled(format!("${:.4}", summary.total_cost), Style::default().fg(theme::GREEN)),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Avg Cost/Req:    ", Style::default().fg(theme::YELLOW)),
                Span::styled(format!("${:.4}", summary.avg_cost_per_request), Style::default().fg(theme::CYAN)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("[t] Change Time Range  [r] Refresh", Style::default().fg(theme::MUTED)),
            ]),
        ];

        let summary_widget = Paragraph::new(summary_lines)
            .block(
                Block::default()
                    .title(" Usage Summary ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::BORDER))
            );
        f.render_widget(summary_widget, chunks[0]);

        // 按 Provider 统计
        let provider_items: Vec<ListItem> = summary.by_provider
            .iter()
            .map(|p| {
                let content = Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{:<18}", p.provider_name), Style::default().fg(theme::YELLOW)),
                    Span::raw("  "),
                    Span::styled(format!("Req: {:>4}", p.requests), Style::default().fg(theme::CYAN)),
                    Span::raw("  "),
                    Span::styled(format!("In: {:>6}", p.input_tokens), Style::default().fg(theme::CYAN)),
                    Span::raw("  "),
                    Span::styled(format!("Out: {:>6}", p.output_tokens), Style::default().fg(theme::MAGENTA)),
                    Span::raw("  "),
                    Span::styled(format!("Total: {:>7}", p.tokens), Style::default().fg(theme::GREEN)),
                    Span::raw("  "),
                    Span::styled(format!("${:>6.2}", p.cost), Style::default().fg(theme::YELLOW)),
                ]);
                ListItem::new(content)
            })
            .collect();

        let provider_list = List::new(provider_items)
            .block(
                Block::default()
                    .title(" By Provider ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::BORDER))
            );
        f.render_widget(provider_list, chunks[1]);
    } else {
        // 没有统计数据
        let info = Paragraph::new("No usage data available.\n\nPress [r] to refresh.")
            .block(
                Block::default()
                    .title(" Usage Statistics ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::BORDER))
            );
        f.render_widget(info, area);
    }
}

fn render_settings(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);

    // 左侧：设置项列表（按功能分类）
    let settings_items = vec![
        // === Logging ===
        ("", "=== Logging ===".to_string()),
        ("Log Level", app.config.log.level.clone()),
        ("Log File", app.config.log.file.clone().unwrap_or_else(|| "(none)".to_string())),
        ("Log Max Size (MB)", app.config.log.max_size_mb.to_string()),
        ("Log Max Backups", app.config.log.max_backups.to_string()),

        // === Database ===
        ("", "=== Database ===".to_string()),
        ("Database Path", app.config.database.path.clone()),
        ("WAL Mode", if app.config.database.wal_mode { "Enabled" } else { "Disabled" }.to_string()),
        ("Max Request Logs", app.config.database.max_request_logs.to_string()),
        ("Archive Directory", app.config.database.archive_dir.clone()),
        ("Auto Cleanup", if app.config.database.auto_cleanup { "Enabled" } else { "Disabled" }.to_string()),

        // === Proxy ===
        ("", "=== Proxy ===".to_string()),
        ("Proxy Port", app.config.proxy.port.to_string()),
        ("Proxy Bind", app.config.proxy.bind.clone()),
        ("Request Timeout", format!("{}s", app.config.proxy.timeout_secs)),

        // === Streaming Timeout ===
        ("", "=== Streaming Timeout ===".to_string()),
        ("First Byte Timeout", format!("{}s", app.config.proxy.streaming_timeout.first_byte_secs)),
        ("Idle Timeout", format!("{}s", app.config.proxy.streaming_timeout.idle_secs)),
        ("Total Timeout", format!("{}s", app.config.proxy.streaming_timeout.total_secs)),

        // === Health Check ===
        ("", "=== Health Check ===".to_string()),
        ("Health Interval", format!("{}s", app.config.health_check.interval_secs)),

        // === Circuit Breaker ===
        ("", "=== Circuit Breaker ===".to_string()),
        ("CB Fail Threshold", app.config.circuit_breaker.failure_threshold.to_string()),
        ("CB Success Threshold", app.config.circuit_breaker.success_threshold.to_string()),
        ("CB Timeout", format!("{}s", app.config.circuit_breaker.timeout_secs)),
        ("CB Half-Open Timeout", format!("{}s", app.config.circuit_breaker.half_open_timeout_secs)),

        // === Model Fallback ===
        ("", "=== Model Fallback ===".to_string()),
        ("Model Fallback", if app.config.model_fallback.enabled { "Enabled" } else { "Disabled" }.to_string()),
    ];

    let items: Vec<ListItem> = settings_items
        .iter()
        .enumerate()
        .map(|(i, (label, value))| {
            // 分类标题行（不可选择）
            if label.is_empty() {
                let content = Line::from(vec![
                    Span::raw("  "),
                    Span::styled(value.as_str(), Style::default().fg(theme::MAGENTA).add_modifier(Modifier::BOLD)),
                ]);
                return ListItem::new(content);
            }

            let selected = i == app.settings_selected;
            let style = if selected {
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let marker = if selected { ">" } else { " " };

            let content = Line::from(vec![
                Span::styled(marker, style),
                Span::raw(" "),
                Span::styled(format!("{:<25}", label), Style::default().fg(theme::YELLOW)),
                Span::raw("  "),
                Span::styled(value.as_str(), style),
            ]);

            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Settings ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER))
        )
        .highlight_style(Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD));

    // 使用 StatefulWidget 来支持滚动
    use ratatui::widgets::{ListState, StatefulWidget};
    let mut state = ListState::default();
    state.select(Some(app.settings_selected));

    f.render_stateful_widget(list, chunks[0], &mut state);

    // 右侧：配置文件信息和操作
    let config_path = crate::config::AppConfig::config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let info_lines = vec![
        Line::from(""),
        Line::from(Span::styled("Config File", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(format!("  {}", config_path)),
        Line::from(""),
        Line::from(Span::styled("Logging", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(format!("  Level: {}", app.config.log.level)),
        Line::from(format!("  File: {}", app.config.log.file.clone().unwrap_or_else(|| "(none)".to_string()))),
        Line::from(format!("  Max Size: {} MB", app.config.log.max_size_mb)),
        Line::from(format!("  Max Backups: {}", app.config.log.max_backups)),
        Line::from(""),
        Line::from(Span::styled("Auto Sync", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(format!("  {}", if app.config.general.auto_sync { "Enabled" } else { "Disabled" })),
        Line::from(""),
        Line::from(Span::styled("Database Info", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(format!("  WAL Mode: {}", if app.config.database.wal_mode { "Enabled" } else { "Disabled" })),
        Line::from(format!("  Max Logs: {}", app.config.database.max_request_logs)),
        Line::from(format!("  Auto Cleanup: {}", if app.config.database.auto_cleanup { "Enabled" } else { "Disabled" })),
        Line::from(""),
        Line::from(Span::styled("Streaming Timeout", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(format!("  First Byte: {}s", app.config.proxy.streaming_timeout.first_byte_secs)),
        Line::from(format!("  Idle: {}s", app.config.proxy.streaming_timeout.idle_secs)),
        Line::from(format!("  Total: {}s", app.config.proxy.streaming_timeout.total_secs)),
        Line::from(""),
        Line::from(Span::styled("Circuit Breaker", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(format!("  Fail: {} | Success: {}",
            app.config.circuit_breaker.failure_threshold,
            app.config.circuit_breaker.success_threshold)),
        Line::from(format!("  Timeout: {}s | Half-Open: {}s",
            app.config.circuit_breaker.timeout_secs,
            app.config.circuit_breaker.half_open_timeout_secs)),
        Line::from(""),
        Line::from(Span::styled("Model Fallback", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(format!("  Status: {}", if app.config.model_fallback.enabled { "Enabled" } else { "Disabled" })),
        Line::from(format!("  Chains: {} configured", app.config.model_fallback.fallback_chains.len())),
        Line::from(""),
        Line::from(Span::styled(
            "[e] Edit  [↑↓] Navigate  [Space] Toggle",
            Style::default().fg(theme::MUTED),
        )),
    ];

    let info = Paragraph::new(info_lines)
        .block(
            Block::default()
                .title(" Info ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER))
        );
    f.render_widget(info, chunks[1]);
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let status = Line::from(vec![
        Span::styled(" Ready ", Style::default().fg(theme::GREEN)),
        Span::styled("│", Style::default().fg(theme::MUTED)),
        Span::styled(format!(" {} providers ", app.providers.len()), Style::default().fg(theme::TEXT)),
        Span::styled("│", Style::default().fg(theme::MUTED)),
        Span::styled(" [q] Quit  [?] Help  [s] Sync Settings ", Style::default().fg(theme::MUTED)),
    ]);

    let paragraph = Paragraph::new(status)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme::BORDER)));
    f.render_widget(paragraph, area);
}

fn render_notification(f: &mut Frame, notification: &crate::app::Notification) {
    use crate::app::NotificationLevel;

    let color = match notification.level {
        NotificationLevel::Success => theme::GREEN,
        NotificationLevel::Warning => theme::YELLOW,
        NotificationLevel::Error => theme::RED,
        NotificationLevel::Info => theme::BLUE,
    };

    // 显示在顶部（标题栏下方）
    let area = Rect {
        x: f.area().width / 4,
        y: 4,  // 标题栏(3) + 标签页(3) + 1 行间距 = 7，这里设为 4 更靠近顶部
        width: f.area().width / 2,
        height: 3,
    };

    let text = Paragraph::new(notification.message.as_str())
        .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(color)));

    f.render_widget(ratatui::widgets::Clear, area);
    f.render_widget(text, area);
}

fn mask_api_key(key: &str) -> String {
    if key.len() <= 8 {
        "*".repeat(key.len())
    } else {
        format!("{}...{}", &key[..4], &key[key.len()-4..])
    }
}
