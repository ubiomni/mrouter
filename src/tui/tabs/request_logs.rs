//! 请求日志 UI 组件

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::app::App;
use crate::models::ProxyRequestLog;
use crate::tui::theme;

/// 渲染请求日志标签页
pub fn render(f: &mut Frame, app: &App, area: Rect) {
    if app.show_log_detail {
        render_log_detail(f, app, area);
    } else {
        render_log_list(f, app, area);
    }
}

/// 渲染日志列表
fn render_log_list(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // 标题和统计
            Constraint::Min(0),     // 日志列表
            Constraint::Length(3),  // 帮助信息
        ])
        .split(area);

    // 标题和统计
    let total_pages = app.get_total_logs_pages().unwrap_or(1);
    let current_page = app.logs_page + 1;

    let title_text = vec![
        Line::from(vec![
            Span::styled("Request Logs", Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD)),
            Span::raw(" | "),
            Span::styled(format!("Page: {}/{}", current_page, total_pages), Style::default().fg(theme::MAGENTA)),
            Span::raw(" | "),
            Span::styled(format!("Showing: {}", app.request_logs.len()), Style::default().fg(theme::YELLOW)),
            Span::raw(" | "),
            Span::styled(format!("Selected: {}/{}", app.selected_log + 1, app.request_logs.len()), Style::default().fg(theme::GREEN)),
        ]),
    ];
    let title = Paragraph::new(title_text)
        .block(Block::default().borders(Borders::ALL).title("📋 Request Logs"));
    f.render_widget(title, chunks[0]);

    // 日志列表
    if app.request_logs.is_empty() {
        let empty_msg = Paragraph::new("No request logs found.\n\nPress 'r' to refresh.")
            .style(Style::default().fg(theme::MUTED))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(empty_msg, chunks[1]);
    } else {
        render_logs_table(f, app, chunks[1]);
    }

    // 帮助信息
    let help_text = vec![
        Line::from(vec![
            Span::styled("↑/↓", Style::default().fg(theme::YELLOW)),
            Span::raw(": Navigate | "),
            Span::styled("←/→", Style::default().fg(theme::YELLOW)),
            Span::raw(": Page | "),
            Span::styled("Enter", Style::default().fg(theme::YELLOW)),
            Span::raw(": Detail | "),
            Span::styled("r", Style::default().fg(theme::YELLOW)),
            Span::raw(": Refresh | "),
            Span::styled("q", Style::default().fg(theme::YELLOW)),
            Span::raw(": Back"),
        ]),
    ];
    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[2]);
}

/// 渲染日志表格
fn render_logs_table(f: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec![
        "Time",
        "Provider",
        "Model",
        "Dur",
        "TTFT",
        "In",
        "Out",
        "Cache",
        "Total",
        "Cost",
        "St",
    ])
    .style(Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let rows: Vec<Row> = app.request_logs.iter().enumerate().map(|(i, log)| {
        let style = if i == app.selected_log {
            Style::default().bg(theme::HIGHLIGHT_BG).fg(theme::TEXT)
        } else {
            Style::default()
        };

        let time = log.request_time.format("%H:%M:%S").to_string();
        let provider = app.get_provider_name(log.provider_id);
        let model = log.model.as_deref().unwrap_or("unknown");
        let duration = log.duration_ms.map(|d| format!("{}ms", d)).unwrap_or_else(|| "-".to_string());
        let ttft = log.first_token_ms.map(|t| format!("{}ms", t)).unwrap_or_else(|| "-".to_string());
        let input = format!("{}", log.input_tokens);
        let output = format!("{}", log.output_tokens);

        // Cache token 显示：如果有缓存，显示 "R:xxx/W:xxx"，否则显示 "-"
        let cache = if log.cache_read_tokens > 0 || log.cache_creation_tokens > 0 {
            if log.cache_read_tokens > 0 && log.cache_creation_tokens > 0 {
                format!("R:{}/W:{}", log.cache_read_tokens, log.cache_creation_tokens)
            } else if log.cache_read_tokens > 0 {
                format!("R:{}", log.cache_read_tokens)
            } else {
                format!("W:{}", log.cache_creation_tokens)
            }
        } else {
            "-".to_string()
        };

        let total = format!("{}", log.total_tokens);
        let cost = format!("${:.4}", log.estimated_cost.max(0.0));
        let status = log.status_code.map(|s| s.to_string()).unwrap_or_else(|| "-".to_string());

        Row::new(vec![time, provider, model.to_string(), duration, ttft, input, output, cache, total, cost, status])
            .style(style)
    }).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(9),   // Time
            Constraint::Length(12),  // Provider
            Constraint::Length(18),  // Model
            Constraint::Length(7),   // Duration
            Constraint::Length(7),   // TTFT
            Constraint::Length(6),   // Input
            Constraint::Length(6),   // Output
            Constraint::Length(12),  // Cache (R:xxx/W:xxx)
            Constraint::Length(7),   // Total
            Constraint::Length(9),   // Cost
            Constraint::Length(5),   // Status
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title("Logs"))
    .highlight_style(Style::default().bg(theme::MUTED).fg(theme::TEXT));

    // 使用 StatefulWidget 来支持滚动
    use ratatui::widgets::{StatefulWidget, TableState};
    let mut state = TableState::default();
    state.select(Some(app.selected_log));

    f.render_stateful_widget(table, area, &mut state);
}

/// 渲染日志详情
fn render_log_detail(f: &mut Frame, app: &App, area: Rect) {
    if let Some(log) = app.get_selected_log() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),     // 详情内容
                Constraint::Length(3),  // 帮助信息
            ])
            .split(area);

        // 详情内容
        let detail_text = format_log_detail(log);
        let total_lines = detail_text.len();
        let visible_lines = chunks[0].height.saturating_sub(2) as usize; // 减去边框

        // 计算滚动位置（确保不超出范围）
        let scroll_offset = app.log_detail_scroll.min(total_lines.saturating_sub(visible_lines));

        let detail = Paragraph::new(detail_text)
            .block(Block::default().borders(Borders::ALL).title(format!(
                "📄 Request Log Detail (Line {}/{})",
                scroll_offset + 1,
                total_lines
            )))
            .wrap(Wrap { trim: true })
            .scroll((scroll_offset as u16, 0));
        f.render_widget(detail, chunks[0]);

        // 帮助信息
        let help_text = vec![
            Line::from(vec![
                Span::styled("Esc/q", Style::default().fg(theme::YELLOW)),
                Span::raw(": Back to List | "),
                Span::styled("↑/↓", Style::default().fg(theme::YELLOW)),
                Span::raw(": Scroll"),
            ]),
        ];
        let help = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(help, chunks[1]);
    }
}

/// 格式化日志详情
fn format_log_detail(log: &ProxyRequestLog) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // 基本信息
    lines.push(Line::from(vec![
        Span::styled("Request ID: ", Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD)),
        Span::raw(log.id.to_string()),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Provider ID: ", Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD)),
        Span::raw(log.provider_id.to_string()),
    ]));

    lines.push(Line::from(""));

    // 时间信息
    lines.push(Line::from(vec![
        Span::styled("Request Time: ", Style::default().fg(theme::GREEN).add_modifier(Modifier::BOLD)),
        Span::raw(log.request_time.format("%Y-%m-%d %H:%M:%S").to_string()),
    ]));

    if let Some(response_time) = log.response_time {
        lines.push(Line::from(vec![
            Span::styled("Response Time: ", Style::default().fg(theme::GREEN).add_modifier(Modifier::BOLD)),
            Span::raw(response_time.format("%Y-%m-%d %H:%M:%S").to_string()),
        ]));
    }

    if let Some(duration) = log.duration_ms {
        lines.push(Line::from(vec![
            Span::styled("Duration: ", Style::default().fg(theme::GREEN).add_modifier(Modifier::BOLD)),
            Span::raw(format!("{}ms", duration)),
        ]));
    }

    if let Some(ttft) = log.first_token_ms {
        lines.push(Line::from(vec![
            Span::styled("TTFT: ", Style::default().fg(theme::GREEN).add_modifier(Modifier::BOLD)),
            Span::raw(format!("{}ms", ttft)),
        ]));
    }

    lines.push(Line::from(""));

    // 请求信息
    if let Some(ref model) = log.model {
        lines.push(Line::from(vec![
            Span::styled("Model: ", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD)),
            Span::raw(model.clone()),
        ]));
    }

    if let Some(ref path) = log.request_path {
        lines.push(Line::from(vec![
            Span::styled("Path: ", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD)),
            Span::raw(path.clone()),
        ]));
    }

    if let Some(ref method) = log.request_method {
        lines.push(Line::from(vec![
            Span::styled("Method: ", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD)),
            Span::raw(method.clone()),
        ]));
    }

    if let Some(status) = log.status_code {
        let status_color = if status >= 200 && status < 300 {
            theme::GREEN
        } else if status >= 400 {
            theme::RED
        } else {
            theme::YELLOW
        };
        lines.push(Line::from(vec![
            Span::styled("Status: ", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD)),
            Span::styled(status.to_string(), Style::default().fg(status_color)),
        ]));
    }

    if let Some(ref session_id) = log.session_id {
        lines.push(Line::from(vec![
            Span::styled("Session ID: ", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD)),
            Span::raw(session_id.clone()),
        ]));
    }

    lines.push(Line::from(""));

    // Token 使用量
    lines.push(Line::from(vec![
        Span::styled("Token Usage:", Style::default().fg(theme::MAGENTA).add_modifier(Modifier::BOLD)),
    ]));

    lines.push(Line::from(vec![
        Span::raw("  Input Tokens: "),
        Span::styled(log.input_tokens.to_string(), Style::default().fg(theme::CYAN)),
    ]));

    lines.push(Line::from(vec![
        Span::raw("  Output Tokens: "),
        Span::styled(log.output_tokens.to_string(), Style::default().fg(theme::CYAN)),
    ]));

    if log.cache_creation_tokens > 0 {
        lines.push(Line::from(vec![
            Span::raw("  Cache Creation: "),
            Span::styled(log.cache_creation_tokens.to_string(), Style::default().fg(theme::YELLOW)),
        ]));
    }

    if log.cache_read_tokens > 0 {
        lines.push(Line::from(vec![
            Span::raw("  Cache Read: "),
            Span::styled(log.cache_read_tokens.to_string(), Style::default().fg(theme::GREEN)),
        ]));
    }

    lines.push(Line::from(vec![
        Span::raw("  Total Tokens: "),
        Span::styled(log.total_tokens.to_string(), Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD)),
    ]));

    lines.push(Line::from(""));

    // 成本明细
    lines.push(Line::from(vec![
        Span::styled("Cost Breakdown:", Style::default().fg(theme::RED).add_modifier(Modifier::BOLD)),
    ]));

    // 计算各项成本的百分比和详细金额
    let total_cost = log.estimated_cost;

    // 简化的成本估算（基于 token 比例）
    // 注意：这是近似值，实际成本需要从 provider 的定价配置计算
    let total_tokens_f64 = log.total_tokens as f64;

    if total_tokens_f64 > 0.0 && total_cost > 0.0 {
        // 计算各类 token 的成本占比（简化估算）
        let billable_input = log.input_tokens.saturating_sub(log.cache_read_tokens);

        // 假设输入:输出价格比为 1:5（Anthropic Claude 的典型比例）
        // 缓存读取价格约为输入的 1/10
        // 缓存创建价格约为输入的 1.25 倍
        let weight_input = billable_input as f64 * 1.0;
        let weight_output = log.output_tokens as f64 * 5.0;
        let weight_cache_read = log.cache_read_tokens as f64 * 0.1;
        let weight_cache_creation = log.cache_creation_tokens as f64 * 1.25;
        let total_weight = weight_input + weight_output + weight_cache_read + weight_cache_creation;

        if total_weight > 0.0 {
            let input_cost = (weight_input / total_weight) * total_cost;
            let output_cost = (weight_output / total_weight) * total_cost;
            let cache_read_cost = (weight_cache_read / total_weight) * total_cost;
            let cache_creation_cost = (weight_cache_creation / total_weight) * total_cost;

            // 显示各项成本
            if billable_input > 0 {
                let pct = (input_cost / total_cost) * 100.0;
                lines.push(Line::from(vec![
                    Span::raw("  Input: "),
                    Span::styled(format!("${:.6}", input_cost), Style::default().fg(theme::CYAN)),
                    Span::raw(format!(" ({:.1}%)", pct)),
                    Span::styled(format!(" [{} tokens]", billable_input), Style::default().fg(theme::MUTED)),
                ]));
            }

            if log.output_tokens > 0 {
                let pct = (output_cost / total_cost) * 100.0;
                lines.push(Line::from(vec![
                    Span::raw("  Output: "),
                    Span::styled(format!("${:.6}", output_cost), Style::default().fg(theme::MAGENTA)),
                    Span::raw(format!(" ({:.1}%)", pct)),
                    Span::styled(format!(" [{} tokens]", log.output_tokens), Style::default().fg(theme::MUTED)),
                ]));
            }

            if log.cache_creation_tokens > 0 {
                let pct = (cache_creation_cost / total_cost) * 100.0;
                lines.push(Line::from(vec![
                    Span::raw("  Cache Write: "),
                    Span::styled(format!("${:.6}", cache_creation_cost), Style::default().fg(theme::YELLOW)),
                    Span::raw(format!(" ({:.1}%)", pct)),
                    Span::styled(format!(" [{} tokens]", log.cache_creation_tokens), Style::default().fg(theme::MUTED)),
                ]));
            }

            if log.cache_read_tokens > 0 {
                let pct = (cache_read_cost / total_cost) * 100.0;
                let savings = (log.cache_read_tokens as f64 * 1.0 / 1_000_000.0) * 3.0 - cache_read_cost;
                lines.push(Line::from(vec![
                    Span::raw("  Cache Read: "),
                    Span::styled(format!("${:.6}", cache_read_cost), Style::default().fg(theme::GREEN)),
                    Span::raw(format!(" ({:.1}%)", pct)),
                    Span::styled(format!(" [{} tokens]", log.cache_read_tokens), Style::default().fg(theme::MUTED)),
                ]));
                if savings > 0.0 {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(format!("💰 Saved: ${:.6}", savings), Style::default().fg(theme::GREEN).add_modifier(Modifier::ITALIC)),
                    ]));
                }
            }
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Total Cost: ", Style::default().fg(theme::RED).add_modifier(Modifier::BOLD)),
        Span::styled(format!("${:.6}", log.estimated_cost.max(0.0)), Style::default().fg(theme::RED).add_modifier(Modifier::BOLD)),
    ]));

    // 错误信息
    if let Some(ref error) = log.error_message {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Error: ", Style::default().fg(theme::RED).add_modifier(Modifier::BOLD)),
        ]));
        lines.push(Line::from(vec![
            Span::styled(error.clone(), Style::default().fg(theme::RED)),
        ]));
    }

    lines
}
