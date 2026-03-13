// MRouter - Terminal-based Model Router for AI CLI tools
// Main entry point

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, EnableBracketedPaste, DisableBracketedPaste},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use tui::widgets::dialog::DialogKind;

mod app;
mod cli;
mod config;
mod daemon;
mod database;
mod logger;
mod models;
mod services;
mod tui;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
    // 加载配置
    let cfg = config::AppConfig::load()?;

    // 初始化日志系统（支持日志滚动）
    logger::init_logger(
        cfg.log.file.as_deref(),
        &cfg.log.level,
        cfg.log.max_size_mb,
        cfg.log.max_backups,
    )?;

    // 解析 CLI 参数
    let args = cli::parse_args();

    // 根据命令执行不同操作
    match args.command {
        None | Some(cli::Command::Tui) => {
            run_tui().await?;
        }
        Some(cli::Command::Daemon(daemon_cmd)) => {
            daemon::handle_command(daemon_cmd).await?;
        }
        Some(cli::Command::Switch { provider }) => {
            cli::commands::switch_provider(&provider).await?;
        }
        Some(cli::Command::List) => {
            cli::commands::list_providers().await?;
        }
        Some(cli::Command::Status) => {
            cli::commands::show_status().await?;
        }
        Some(cli::Command::Health) => {
            cli::commands::health_check().await?;
        }
        Some(cli::Command::Stats { export }) => {
            cli::commands::show_stats(export).await?;
        }
        Some(cli::Command::Proxy(proxy_cmd)) => {
            cli::commands::handle_proxy(proxy_cmd).await?;
        }
    }

    Ok(())
}

async fn run_tui() -> Result<()> {
    // 加载配置
    let cfg = config::AppConfig::load()?;

    // 初始化数据库
    let db = database::init_with_config(&cfg).await?;

    // 创建应用状态
    let mut app = app::App::new(db, cfg).await?;

    // 设置终端
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 运行应用
    let res = run_app(&mut terminal, &mut app).await;

    // 恢复终端
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut app::App,
) -> Result<()> {
    loop {
        // 渲染 UI
        terminal.draw(|f| tui::ui::render(f, app))?;

        // 处理事件
        if let Some(action) = tui::event::handle_events(app).await? {
            match action {
                tui::event::Action::Quit => break,
                tui::event::Action::Refresh => {
                    app.refresh().await?;
                }
                tui::event::Action::SwitchTab(tab) => {
                    app.current_tab = tab;

                    // 切换到 Stats 标签页时自动加载统计数据
                    if tab == app::Tab::Stats {
                        if let Err(e) = app.refresh_stats() {
                            app.show_notification(format!("Failed to load stats: {}", e), app::NotificationLevel::Error);
                        }
                    }

                    // 切换到 RequestLogs 标签页时自动加载日志
                    if tab == app::Tab::RequestLogs {
                        if let Err(e) = app.refresh_request_logs() {
                            app.show_notification(format!("Failed to load logs: {}", e), app::NotificationLevel::Error);
                        }
                    }
                }
                tui::event::Action::Navigate(dir) => {
                    app.navigate(dir);
                }
                tui::event::Action::Select => {
                    app.handle_select().await?;
                }
                tui::event::Action::Edit => {
                    app.handle_edit().await?;
                }
                tui::event::Action::Delete => {
                    app.handle_delete().await?;
                }
                tui::event::Action::Add => {
                    app.handle_add().await?;
                }
                tui::event::Action::ManageSyncSettings => {
                    app.manage_sync_settings().await?;
                }
                tui::event::Action::ToggleHelp => {
                    app.show_help = !app.show_help;
                }
                tui::event::Action::ConfirmYes => {
                    app.handle_confirm_yes().await?;
                }
                tui::event::Action::ConfirmNo | tui::event::Action::DialogCancel => {
                    // 如果是同步模式选择，ConfirmNo = Direct 模式
                    if app.pending_sync_cli_tools.is_some() {
                        app.handle_sync_direct().await?;
                    } else if matches!(app.dialog, Some(tui::widgets::dialog::DialogKind::ModelListViewer { .. })) {
                        // 如果是模型浏览窗口，恢复之前的对话框
                        app.dialog = app.previous_dialog.take();
                    } else {
                        app.dialog = None;
                        app.input_mode = app::InputMode::Normal;
                    }
                }
                tui::event::Action::InputSubmit => {
                    app.handle_input_submit().await?;
                }
                tui::event::Action::InputChar(c) => {
                    app.handle_input_char(c);
                }
                tui::event::Action::InputPaste(text) => {
                    app.handle_input_paste(&text);
                }
                tui::event::Action::InputPasteClipboard => {
                    app.handle_input_paste_clipboard();
                }
                tui::event::Action::InputBackspace => {
                    app.handle_input_backspace();
                }
                tui::event::Action::InputNextField => {
                    app.handle_input_next_field();
                }
                tui::event::Action::InputPrevField => {
                    app.handle_input_prev_field();
                }
                tui::event::Action::MouseClickField(field_index) => {
                    app.handle_mouse_click_field(field_index);
                }
                tui::event::Action::InputSelectNext => {
                    if matches!(app.dialog, Some(DialogKind::MultiSelect { .. })) {
                        app.handle_multiselect_navigate(false);
                    } else {
                        app.handle_input_select_next();
                    }
                }
                tui::event::Action::InputSelectPrev => {
                    if matches!(app.dialog, Some(DialogKind::MultiSelect { .. })) {
                        app.handle_multiselect_navigate(true);
                    } else {
                        app.handle_input_select_prev();
                    }
                }
                tui::event::Action::InputClear => {
                    app.handle_input_clear();
                }
                tui::event::Action::InputCopy => {
                    app.handle_input_copy();
                }
                tui::event::Action::InputTogglePassword => {
                    app.handle_input_toggle_password();
                }
                tui::event::Action::InputScrollLeft => {
                    app.handle_input_scroll_left();
                }
                tui::event::Action::InputScrollRight => {
                    app.handle_input_scroll_right();
                }
                tui::event::Action::InputOpenModelViewer => {
                    app.handle_open_model_viewer();
                }
                tui::event::Action::ModelViewerScroll(direction) => {
                    app.handle_model_viewer_scroll(direction);
                }
                tui::event::Action::ViewSupportedModels => {
                    app.handle_view_supported_models();
                }
                tui::event::Action::ConfigurePricing => {
                    app.handle_configure_pricing();
                }
                tui::event::Action::ConfigureModelMappings => {
                    app.handle_configure_model_mappings();
                }
                tui::event::Action::ConfigureAuthHeader => {
                    app.handle_configure_auth_header();
                }
                tui::event::Action::InputFetchModels => {
                    app.handle_fetch_models().await?;
                }
                tui::event::Action::MultiSelectToggle => {
                    app.handle_multiselect_toggle();
                }
                tui::event::Action::MultiSelectSubmit => {
                    app.handle_multiselect_submit().await?;
                }
                tui::event::Action::ProxyStart => {
                    app.handle_proxy_start().await?;
                }
                tui::event::Action::ProxyStop => {
                    app.handle_proxy_stop().await?;
                }
                tui::event::Action::ProxyEditPort => {
                    app.handle_proxy_edit_port().await?;
                }
                tui::event::Action::ResetCircuitBreaker => {
                    app.handle_reset_circuit_breaker().await?;
                }
                tui::event::Action::ToggleStatsTimeRange => {
                    app.handle_toggle_stats_time_range()?;
                }
                tui::event::Action::ToggleLogDetail => {
                    app.toggle_log_detail();
                }
                tui::event::Action::ScrollLogDetailUp => {
                    app.scroll_log_detail_up();
                }
                tui::event::Action::ScrollLogDetailDown => {
                    app.scroll_log_detail_down();
                }
                tui::event::Action::NextLogsPage => {
                    if let Err(e) = app.next_logs_page() {
                        app.show_notification(format!("Failed to load next page: {}", e), app::NotificationLevel::Error);
                    }
                }
                tui::event::Action::PreviousLogsPage => {
                    if let Err(e) = app.previous_logs_page() {
                        app.show_notification(format!("Failed to load previous page: {}", e), app::NotificationLevel::Error);
                    }
                }
            }
        }

        // 清理过期通知
        app.clear_old_notifications();
    }

    Ok(())
}
