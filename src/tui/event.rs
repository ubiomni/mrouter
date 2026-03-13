// 事件处理

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use std::time::Duration;
use crate::app::{App, InputMode, Tab};
use crate::tui::widgets::dialog::DialogKind;

pub enum Action {
    Quit,
    Refresh,
    SwitchTab(Tab),
    Navigate(Direction),
    Select,
    Edit,
    Delete,
    Add,
    ManageSyncSettings,
    ToggleHelp,
    ConfirmYes,
    ConfirmNo,
    InputSubmit,
    DialogCancel,
    InputChar(char),
    InputPaste(String),
    InputBackspace,
    InputNextField,
    InputPrevField,
    InputSelectNext,
    InputSelectPrev,
    InputClear,
    InputTogglePassword,
    InputCopy,            // 复制当前字段的值到剪贴板
    InputPasteClipboard,  // 从剪贴板粘贴（Ctrl+V fallback）
    InputFetchModels,     // 手动拉取模型列表
    InputScrollLeft,      // 文本字段向左滚动
    InputScrollRight,     // 文本字段向右滚动
    InputOpenModelViewer, // 打开模型列表浏览窗口
    ModelViewerScroll(Direction), // 模型列表浏览窗口滚动
    ViewSupportedModels,  // 在 Provider 详情页查看支持的模型列表
    ConfigurePricing,     // 配置 Provider 定价
    ConfigureModelMappings, // 配置模型名称映射
    ConfigureAuthHeader,  // 配置鉴权 Header
    MultiSelectToggle,
    MultiSelectSubmit,
    ProxyStart,
    ProxyStop,
    ProxyEditPort,
    ResetCircuitBreaker,  // 重置所有 Circuit Breaker
    ToggleStatsTimeRange, // 切换统计时间范围
    MouseClickField(usize), // 鼠标点击字段切换 focus
    ToggleLogDetail,      // 切换日志详情显示
    ScrollLogDetailUp,    // 日志详情向上滚动
    ScrollLogDetailDown,  // 日志详情向下滚动
    NextLogsPage,         // 下一页日志
    PreviousLogsPage,     // 上一页日志
}

pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

pub async fn handle_events(app: &mut App) -> Result<Option<Action>> {
    if event::poll(Duration::from_millis(100))? {
        let ev = event::read()?;

        // 处理粘贴事件（支持中文 IME 输入）
        if let Event::Paste(text) = ev {
            if app.input_mode == InputMode::Editing || app.dialog.is_some() {
                return Ok(Some(Action::InputPaste(text)));
            }
            return Ok(None);
        }

        // 处理鼠标事件（支持点击切换 focus）
        if let Event::Mouse(mouse) = ev {
            if let MouseEventKind::Down(_) = mouse.kind {
                // 如果在输入对话框中，尝试根据鼠标位置切换字段
                if let Some(DialogKind::Input { fields, focused_field, .. }) = &app.dialog {
                    // 获取终端大小
                    let terminal_size = crossterm::terminal::size().unwrap_or((80, 24));
                    let terminal_height = terminal_size.1;

                    // 计算对话框的实际高度和可见字段
                    let has_active_select = fields.get(*focused_field).map_or(false, |f| f.is_select());
                    let dropdown_rows: u16 = if has_active_select { 8 } else { 0 };
                    let desired_height = (fields.len() as u16 * 3) + 5 + dropdown_rows;
                    let available_height = terminal_height.saturating_sub(4);
                    let dialog_height = desired_height.min(available_height);

                    let dialog_top = (terminal_height.saturating_sub(dialog_height)) / 2;

                    // 对话框内部起始位置（去掉标题边框）
                    let inner_top = dialog_top + 1;
                    let inner_height = dialog_height.saturating_sub(2) as usize;

                    // 计算滚动偏移（与 render_input_dialog 中的逻辑一致）
                    let field_height = 3;
                    let max_visible_fields = (inner_height.saturating_sub(2)) / field_height;
                    let scroll_offset = if fields.len() > max_visible_fields {
                        if *focused_field >= max_visible_fields {
                            focused_field.saturating_sub(max_visible_fields / 2).min(fields.len().saturating_sub(max_visible_fields))
                        } else {
                            0
                        }
                    } else {
                        0
                    };

                    // 计算相对于对话框内部的行号
                    let relative_row = (mouse.row as u16).saturating_sub(inner_top);

                    // 每个字段占 3 行，加上滚动偏移
                    let visible_field_index = (relative_row / 3) as usize;
                    let actual_field_index = visible_field_index + scroll_offset;

                    if actual_field_index < fields.len() {
                        return Ok(Some(Action::MouseClickField(actual_field_index)));
                    }
                }
            }
            return Ok(None);
        }

        if let Event::Key(key) = ev {
            // 只处理 Press 事件，忽略 Release 和 Repeat
            // Windows 上 crossterm 会对每个按键同时发送 Press 和 Release，导致重复输入
            if key.kind != KeyEventKind::Press {
                return Ok(None);
            }

            // 帮助对话框优先处理
            if app.show_help {
                return match key.code {
                    KeyCode::Esc | KeyCode::Char('?') | KeyCode::F(1) => {
                        Ok(Some(Action::ToggleHelp))
                    }
                    _ => Ok(None),
                };
            }

            // 对话框模式优先处理
            if let Some(dialog) = &app.dialog {
                return match dialog {
                    DialogKind::Confirm { .. } => handle_confirm_mode(key),
                    DialogKind::Input { .. } => handle_input_mode(key, app),
                    DialogKind::MultiSelect { .. } => handle_multiselect_mode(key),
                    DialogKind::ModelListViewer { .. } => handle_model_viewer_mode(key),
                    DialogKind::Help => {
                        if key.code == KeyCode::Esc {
                            Ok(Some(Action::DialogCancel))
                        } else {
                            Ok(None)
                        }
                    }
                };
            }

            return match app.input_mode {
                InputMode::Normal => handle_normal_mode(key, app),
                InputMode::Editing => handle_input_mode(key, app),
                InputMode::Searching => handle_searching_mode(key, app),
            };
        }
    }
    Ok(None)
}

fn handle_confirm_mode(key: event::KeyEvent) -> Result<Option<Action>> {
    // 只处理没有 modifier 的按键，避免拦截 Ctrl+Y 等组合键
    if !key.modifiers.is_empty() {
        return Ok(None);
    }

    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => Ok(Some(Action::ConfirmYes)),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Ok(Some(Action::ConfirmNo)),
        _ => Ok(None),
    }
}

fn handle_input_mode(key: event::KeyEvent, app: &App) -> Result<Option<Action>> {
    // 检查当前聚焦的字段是否是选择框和是否是 Supported Models 字段
    let (is_select_field, is_models_field) = if let Some(DialogKind::Input { fields, focused_field, .. }) = &app.dialog {
        let is_select = fields.get(*focused_field).map(|f| f.is_select()).unwrap_or(false);
        let is_models = fields.get(*focused_field).map(|f| f.label.contains("Supported Models")).unwrap_or(false);
        (is_select, is_models)
    } else {
        (false, false)
    };

    match key.code {
        KeyCode::Enter => Ok(Some(Action::InputSubmit)),
        KeyCode::Esc => Ok(Some(Action::DialogCancel)),
        // Space 键：在 Supported Models 字段打开浏览窗口
        KeyCode::Char(' ') if is_models_field => Ok(Some(Action::InputOpenModelViewer)),
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                Ok(Some(Action::InputPrevField))
            } else {
                Ok(Some(Action::InputNextField))
            }
        }
        KeyCode::BackTab => Ok(Some(Action::InputPrevField)),
        KeyCode::Backspace => Ok(Some(Action::InputBackspace)),
        KeyCode::Up => Ok(Some(Action::InputSelectPrev)),
        KeyCode::Down => Ok(Some(Action::InputSelectNext)),
        KeyCode::Left => {
            if is_select_field {
                Ok(Some(Action::InputSelectPrev))
            } else {
                Ok(Some(Action::InputScrollLeft))
            }
        }
        KeyCode::Right => {
            if is_select_field {
                Ok(Some(Action::InputSelectNext))
            } else {
                Ok(Some(Action::InputScrollRight))
            }
        }
        // Ctrl+U 清空当前字段
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => Ok(Some(Action::InputClear)),
        // Ctrl+K 复制当前字段的值（K for Kopy，避免与其他快捷键冲突）
        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Ok(Some(Action::InputCopy))
        }
        // Ctrl+Shift+C 复制当前字段的值（需要同时按 Ctrl+Shift+C）
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) && key.modifiers.contains(KeyModifiers::SHIFT) => {
            Ok(Some(Action::InputCopy))
        }
        KeyCode::Char('C') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Ok(Some(Action::InputCopy))
        }
        // Ctrl+Y 复制当前字段的值（备用，某些终端可能不支持）
        KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Ok(Some(Action::InputCopy))
        }
        // Ctrl+V 从剪贴板粘贴（当 bracketed paste 不可用时的 fallback）
        KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => Ok(Some(Action::InputPasteClipboard)),
        // Ctrl+R 切换密码明文/密文
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => Ok(Some(Action::InputTogglePassword)),
        // Ctrl+F 手动拉取模型列表
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => Ok(Some(Action::InputFetchModels)),
        KeyCode::Char(c) => Ok(Some(Action::InputChar(c))),
        _ => Ok(None),
    }
}

fn handle_normal_mode(key: event::KeyEvent, app: &App) -> Result<Option<Action>> {
    match (key.code, key.modifiers) {
        // 全局快捷键
        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            // 如果在日志详情页面，按 q 返回列表而不是退出
            if app.current_tab == Tab::RequestLogs && app.show_log_detail {
                Ok(Some(Action::ToggleLogDetail))
            } else {
                Ok(Some(Action::Quit))
            }
        }
        (KeyCode::Char('1'), _) => Ok(Some(Action::SwitchTab(Tab::Providers))),
        // (KeyCode::Char('2'), _) => Ok(Some(Action::SwitchTab(Tab::Mcp))),  // 暂时隐藏
        (KeyCode::Char('2'), _) => Ok(Some(Action::SwitchTab(Tab::Proxy))),
        (KeyCode::Char('3'), _) => Ok(Some(Action::SwitchTab(Tab::RequestLogs))),
        (KeyCode::Char('4'), _) => Ok(Some(Action::SwitchTab(Tab::Stats))),  // Stats 标签页
        (KeyCode::Char('5'), _) => Ok(Some(Action::SwitchTab(Tab::Settings))),

        // 帮助
        (KeyCode::Char('?'), _) | (KeyCode::F(1), _) => Ok(Some(Action::ToggleHelp)),

        // 管理同步设置 (s = sync)
        (KeyCode::Char('s'), _) => {
            if app.current_tab == Tab::Providers {
                Ok(Some(Action::ManageSyncSettings))
            } else if app.current_tab == Tab::Proxy {
                Ok(Some(Action::ProxyStart))
            } else {
                Ok(None)
            }
        }

        // Proxy 标签页特殊快捷键
        (KeyCode::Char('p'), _) if app.current_tab == Tab::Proxy => {
            Ok(Some(Action::ProxyStop))
        }
        (KeyCode::Char('e'), _) if app.current_tab == Tab::Proxy => {
            Ok(Some(Action::ProxyEditPort))
        }

        // 导航
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
            // 如果在日志详情页面，向上滚动
            if app.current_tab == Tab::RequestLogs && app.show_log_detail {
                Ok(Some(Action::ScrollLogDetailUp))
            } else {
                Ok(Some(Action::Navigate(Direction::Up)))
            }
        }
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
            // 如果在日志详情页面，向下滚动
            if app.current_tab == Tab::RequestLogs && app.show_log_detail {
                Ok(Some(Action::ScrollLogDetailDown))
            } else {
                Ok(Some(Action::Navigate(Direction::Down)))
            }
        }
        (KeyCode::Left, _) | (KeyCode::Char('h'), _) => {
            // 如果在日志列表页面，向前翻页
            if app.current_tab == Tab::RequestLogs && !app.show_log_detail {
                Ok(Some(Action::PreviousLogsPage))
            } else {
                Ok(Some(Action::Navigate(Direction::Left)))
            }
        }
        (KeyCode::Right, _) | (KeyCode::Char('l'), _) => {
            // 如果在日志列表页面，向后翻页
            if app.current_tab == Tab::RequestLogs && !app.show_log_detail {
                Ok(Some(Action::NextLogsPage))
            } else {
                Ok(Some(Action::Navigate(Direction::Right)))
            }
        }

        // 分页（保留 PageUp/PageDown 支持，兼容外接键盘）
        (KeyCode::PageUp, _) => {
            if app.current_tab == Tab::RequestLogs && !app.show_log_detail {
                Ok(Some(Action::PreviousLogsPage))
            } else {
                Ok(None)
            }
        }
        (KeyCode::PageDown, _) => {
            if app.current_tab == Tab::RequestLogs && !app.show_log_detail {
                Ok(Some(Action::NextLogsPage))
            } else {
                Ok(None)
            }
        },

        // 操作
        (KeyCode::Enter, _) | (KeyCode::Char(' '), _) => {
            if app.current_tab == Tab::RequestLogs {
                Ok(Some(Action::ToggleLogDetail))
            } else {
                Ok(Some(Action::Select))
            }
        }
        (KeyCode::Char('e'), _) => Ok(Some(Action::Edit)),
        (KeyCode::Char('d'), _) => Ok(Some(Action::Delete)),
        (KeyCode::Char('a'), _) => Ok(Some(Action::Add)),
        (KeyCode::Char('v'), _) if app.current_tab == Tab::Providers => {
            Ok(Some(Action::ViewSupportedModels))
        }
        (KeyCode::Char('p'), _) if app.current_tab == Tab::Providers => {
            Ok(Some(Action::ConfigurePricing))
        }
        (KeyCode::Char('m'), _) if app.current_tab == Tab::Providers => {
            Ok(Some(Action::ConfigureModelMappings))
        }
        (KeyCode::Char('o'), _) if app.current_tab == Tab::Providers => {
            Ok(Some(Action::ConfigureAuthHeader))
        }
        (KeyCode::Char('r'), _) => {
            if app.current_tab == Tab::Providers {
                Ok(Some(Action::ResetCircuitBreaker))
            } else {
                Ok(Some(Action::Refresh))
            }
        }
        (KeyCode::Char('t'), _) if app.current_tab == Tab::Stats => {
            Ok(Some(Action::ToggleStatsTimeRange))
        }
        (KeyCode::Esc, _) if app.current_tab == Tab::RequestLogs && app.show_log_detail => {
            Ok(Some(Action::ToggleLogDetail))
        }

        _ => Ok(None),
    }
}

fn handle_searching_mode(_key: event::KeyEvent, _app: &App) -> Result<Option<Action>> {
    Ok(None)
}

fn handle_multiselect_mode(key: event::KeyEvent) -> Result<Option<Action>> {
    match key.code {
        KeyCode::Enter => Ok(Some(Action::MultiSelectSubmit)),
        KeyCode::Esc => Ok(Some(Action::DialogCancel)),
        KeyCode::Char(' ') => Ok(Some(Action::MultiSelectToggle)),
        KeyCode::Up | KeyCode::Char('k') => Ok(Some(Action::InputSelectPrev)),
        KeyCode::Down | KeyCode::Char('j') => Ok(Some(Action::InputSelectNext)),
        _ => Ok(None),
    }
}

fn handle_model_viewer_mode(key: event::KeyEvent) -> Result<Option<Action>> {
    match key.code {
        KeyCode::Esc => Ok(Some(Action::DialogCancel)),
        KeyCode::Up | KeyCode::Char('k') => Ok(Some(Action::ModelViewerScroll(Direction::Up))),
        KeyCode::Down | KeyCode::Char('j') => Ok(Some(Action::ModelViewerScroll(Direction::Down))),
        _ => Ok(None),
    }
}
