// TUI 对话框组件

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};
use crate::tui::theme;

/// 对话框类型
pub enum DialogKind {
    /// 确认对话框
    Confirm {
        title: String,
        message: String,
    },
    /// 输入对话框
    Input {
        title: String,
        fields: Vec<InputField>,
        focused_field: usize,
    },
    /// 多选对话框
    MultiSelect {
        title: String,
        message: String,
        options: Vec<String>,
        selected: Vec<usize>,
        highlighted: usize,
    },
    /// 模型列表浏览窗口
    ModelListViewer {
        title: String,
        models: Vec<String>,
        scroll_offset: usize,
    },
    /// 帮助对话框
    Help,
}

/// 输入字段类型
pub enum FieldKind {
    Text,
    Password { visible: bool },
    /// 复选框
    Checkbox { checked: bool },
    /// 可搜索选择框
    Select {
        options: Vec<String>,
        selected: usize,
        filter: String,
        /// 过滤后的索引列表（指向 options 的下标）
        filtered: Vec<usize>,
        /// 在 filtered 列表中的高亮位置
        highlight: usize,
    },
}

/// 输入字段
pub struct InputField {
    pub label: String,
    pub value: String,
    pub placeholder: String,
    pub kind: FieldKind,
    /// 标记字段是否被用户手动清空（用于阻止自动填充）
    pub manually_cleared: bool,
    /// 水平滚动偏移量（用于长文本浏览）
    pub scroll_offset: usize,
    /// 光标位置（字符索引）
    pub cursor_pos: usize,
}

impl InputField {
    pub fn new(label: &str, placeholder: &str) -> Self {
        Self {
            label: label.to_string(),
            value: String::new(),
            placeholder: placeholder.to_string(),
            kind: FieldKind::Text,
            manually_cleared: false,
            scroll_offset: 0,
            cursor_pos: 0,
        }
    }

    pub fn password(label: &str, placeholder: &str) -> Self {
        Self {
            label: label.to_string(),
            value: String::new(),
            placeholder: placeholder.to_string(),
            kind: FieldKind::Password { visible: false },
            manually_cleared: false,
            scroll_offset: 0,
            cursor_pos: 0,
        }
    }

    pub fn select(label: &str, options: Vec<String>, default: usize) -> Self {
        let value = options.get(default).cloned().unwrap_or_default();
        let filtered: Vec<usize> = (0..options.len()).collect();
        let highlight = filtered.iter().position(|&i| i == default).unwrap_or(0);
        Self {
            label: label.to_string(),
            value,
            placeholder: String::new(),
            kind: FieldKind::Select {
                options,
                selected: default,
                filter: String::new(),
                filtered,
                highlight,
            },
            manually_cleared: false,
            scroll_offset: 0,
            cursor_pos: 0,
        }
    }

    pub fn checkbox(label: &str, checked: bool) -> Self {
        Self {
            label: label.to_string(),
            value: if checked { "true" } else { "false" }.to_string(),
            placeholder: String::new(),
            kind: FieldKind::Checkbox { checked },
            manually_cleared: false,
            scroll_offset: 0,
            cursor_pos: 0,
        }
    }

    pub fn is_select(&self) -> bool {
        matches!(self.kind, FieldKind::Select { .. })
    }

    pub fn is_checkbox(&self) -> bool {
        matches!(self.kind, FieldKind::Checkbox { .. })
    }

    /// 设置字段值并将光标移动到末尾
    pub fn set_value(&mut self, value: String) {
        self.cursor_pos = value.chars().count();
        self.value = value;
    }

    /// 将字符位置（cursor_pos）转换为字节位置，用于 String::insert/remove 等操作
    pub fn cursor_byte_pos(&self) -> usize {
        self.value.char_indices()
            .nth(self.cursor_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len())
    }

    /// 选择框：输入过滤字符
    pub fn select_filter_push(&mut self, c: char) {
        if let FieldKind::Select { options, filter, filtered, highlight, .. } = &mut self.kind {
            filter.push(c);
            Self::refilter(options, filter, filtered, highlight);
        }
    }

    /// 选择框：删除过滤字符
    pub fn select_filter_pop(&mut self) {
        if let FieldKind::Select { options, filter, filtered, highlight, .. } = &mut self.kind {
            filter.pop();
            Self::refilter(options, filter, filtered, highlight);
        }
    }

    /// 重新计算过滤结果（模糊匹配）
    fn refilter(options: &[String], filter: &str, filtered: &mut Vec<usize>, highlight: &mut usize) {
        let query = filter.to_lowercase();
        *filtered = options.iter().enumerate()
            .filter(|(_, name)| {
                if query.is_empty() {
                    return true;
                }
                let name_lower = name.to_lowercase();
                // 模糊匹配：query 的每个字符按顺序出现在 name 中
                let mut chars = query.chars();
                let mut current = chars.next();
                for nc in name_lower.chars() {
                    if let Some(qc) = current {
                        if nc == qc {
                            current = chars.next();
                        }
                    } else {
                        break;
                    }
                }
                current.is_none()
            })
            .map(|(i, _)| i)
            .collect();
        if *highlight >= filtered.len() {
            *highlight = filtered.len().saturating_sub(1);
        }
    }

    /// 选择框：高亮下移
    pub fn select_next(&mut self) {
        if let FieldKind::Select { filtered, highlight, .. } = &mut self.kind {
            if !filtered.is_empty() {
                *highlight = (*highlight + 1) % filtered.len();
            }
        }
    }

    /// 选择框：高亮上移
    pub fn select_prev(&mut self) {
        if let FieldKind::Select { filtered, highlight, .. } = &mut self.kind {
            if !filtered.is_empty() {
                if *highlight == 0 {
                    *highlight = filtered.len() - 1;
                } else {
                    *highlight -= 1;
                }
            }
        }
    }

    /// 选择框：确认当前高亮项
    pub fn select_confirm(&mut self) -> bool {
        if let FieldKind::Select { options, selected, filtered, highlight, filter } = &mut self.kind {
            if let Some(&idx) = filtered.get(*highlight) {
                *selected = idx;
                self.value = options[idx].clone();
                filter.clear();
                *filtered = (0..options.len()).collect();
                *highlight = filtered.iter().position(|&i| i == *selected).unwrap_or(0);
                return true;
            }
        }
        false
    }

    /// 获取选择框的过滤文本
    pub fn select_filter(&self) -> &str {
        if let FieldKind::Select { filter, .. } = &self.kind {
            filter.as_str()
        } else {
            ""
        }
    }

    /// 获取选择框的过滤结果和高亮位置
    pub fn select_state(&self) -> Option<(&[String], &[usize], usize)> {
        if let FieldKind::Select { options, filtered, highlight, .. } = &self.kind {
            Some((options, filtered, *highlight))
        } else {
            None
        }
    }

    /// 获取当前高亮的选项值(用于实时获取选择框的值)
    pub fn get_highlighted_option(&self) -> Option<String> {
        if let FieldKind::Select { options, filtered, highlight, .. } = &self.kind {
            filtered.get(*highlight)
                .and_then(|&idx| options.get(idx))
                .cloned()
        } else {
            None
        }
    }

    /// 切换密码字段明文/密文
    pub fn toggle_password_visibility(&mut self) {
        if let FieldKind::Password { visible } = &mut self.kind {
            *visible = !*visible;
        }
    }

    pub fn is_password(&self) -> bool {
        matches!(self.kind, FieldKind::Password { .. })
    }

    pub fn is_password_visible(&self) -> bool {
        matches!(self.kind, FieldKind::Password { visible: true })
    }

    /// 向右滚动（增加偏移量）
    pub fn scroll_right(&mut self, step: usize) {
        let max_offset = self.value.len().saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + step).min(max_offset);
    }

    /// 向左滚动（减少偏移量）
    pub fn scroll_left(&mut self, step: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(step);
    }

    /// 重置滚动偏移量
    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn display_value(&self) -> String {
        match &self.kind {
            FieldKind::Select { filter, .. } => {
                if filter.is_empty() {
                    format!("✦ {}", self.value)
                } else {
                    filter.clone()
                }
            }
            FieldKind::Password { visible } => {
                if self.value.is_empty() {
                    self.placeholder.clone()
                } else if *visible {
                    self.value.clone()
                } else {
                    "*".repeat(self.value.len())
                }
            }
            FieldKind::Checkbox { checked } => {
                if *checked { "Enabled" } else { "Disabled" }.to_string()
            }
            FieldKind::Text => {
                if self.value.is_empty() {
                    self.placeholder.clone()
                } else {
                    self.value.clone()
                }
            }
        }
    }
}

/// 渲染确认对话框
pub fn render_confirm_dialog(f: &mut Frame, title: &str, message: &str) {
    let area = centered_rect(50, 30, f.area());

    f.render_widget(Clear, area);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::YELLOW));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(inner);

    let msg = Paragraph::new(message)
        .style(Style::default().fg(theme::TEXT));
    f.render_widget(msg, chunks[0]);

    let buttons = Paragraph::new(Line::from(vec![
        Span::styled("  [Y] Yes  ", Style::default().fg(theme::GREEN).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("  [N] No  ", Style::default().fg(theme::RED).add_modifier(Modifier::BOLD)),
    ]));
    f.render_widget(buttons, chunks[1]);
}

/// 渲染输入对话框
/// Provider 对话框中 select 字段的左右布局高度
fn select_split_height(field: &InputField) -> u16 {
    if field.label.contains("Type") { 6 } else { 4 }
}

/// 渲染 select 字段：左侧文本框 + 右侧常驻列表（Provider 对话框专用）
fn render_select_field_split(f: &mut Frame, field: &InputField, is_focused: bool, area: Rect) {
    let border_color = if is_focused { theme::YELLOW } else { theme::MUTED };
    let label_color = if is_focused { theme::YELLOW } else { theme::TEXT };

    // 左右分割: 左 40% 文本框, 右 60% 列表
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(60),
        ])
        .split(area);

    // 左侧: Type 文本框 (占满整个左侧高度)
    let filter = field.select_filter();
    let display = field.display_value();
    let cursor = if is_focused { "▎" } else { "" };
    let value_color = if filter.is_empty() { theme::CYAN } else { theme::TEXT };
    let hint = if is_focused && filter.is_empty() { "\n  ↑↓ select\n  type to filter" } else { "" };

    let input = Paragraph::new(format!(" {}{}{}", display, cursor, hint))
        .style(Style::default().fg(value_color))
        .block(
            Block::default()
                .title(Span::styled(format!(" {} ", field.label), Style::default().fg(label_color)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
        );
    f.render_widget(input, h_chunks[0]);

    // 右侧: Type 列表 (常驻显示)
    if let Some((options, filtered, highlight)) = field.select_state() {
        let list_border_color = if is_focused { theme::YELLOW } else { theme::MUTED };
        let visible_rows = h_chunks[1].height.saturating_sub(2) as usize;
        let total_items = filtered.len();

        let scroll_offset = if highlight >= visible_rows / 2 {
            (highlight - visible_rows / 2).min(total_items.saturating_sub(visible_rows))
        } else {
            0
        };
        let visible_end = (scroll_offset + visible_rows).min(total_items);

        let items: Vec<ListItem> = filtered[scroll_offset..visible_end]
            .iter()
            .enumerate()
            .map(|(vi, &opt_idx)| {
                let fi = scroll_offset + vi;
                let name = &options[opt_idx];
                let is_hl = fi == highlight;
                let is_current = name == &field.value;
                let style = if is_hl && is_focused {
                    Style::default().fg(theme::BASE).bg(theme::CYAN).add_modifier(Modifier::BOLD)
                } else if is_current {
                    Style::default().fg(theme::GREEN).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::TEXT)
                };
                let marker = if is_current { " ● " } else { "   " };
                ListItem::new(Line::from(vec![
                    Span::styled(marker, style),
                    Span::styled(name.as_str(), style),
                ]))
            })
            .collect();

        let match_info = format!(" {}/{} ", filtered.len(), options.len());
        let list = List::new(items)
            .block(
                Block::default()
                    .title(Span::styled(match_info, Style::default().fg(theme::MUTED)))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(list_border_color))
            );
        f.render_widget(list, h_chunks[1]);
    }
}

pub fn render_input_dialog(f: &mut Frame, title: &str, fields: &[InputField], focused: usize) {
    let is_provider_dialog = title.contains("Provider");

    // 只在非 Provider 对话框 聚焦 select 字段时预留下拉列表空间
    let has_active_select = fields.get(focused).map_or(false, |f| f.is_select());
    let dropdown_rows: u16 = if has_active_select && !is_provider_dialog { 8 } else { 0 };

    // 计算总高度
    let fields_height: u16 = if is_provider_dialog {
        fields.iter().map(|f| if f.is_select() { select_split_height(f) } else { 3 }).sum()
    } else {
        fields.len() as u16 * 3
    };
    let desired_height = fields_height + 5 + dropdown_rows;

    // 获取可用高度，确保对话框不会超出屏幕
    let available_height = f.area().height.saturating_sub(4); // 留出边距
    let height = desired_height.min(available_height);

    let area = centered_rect_fixed(65, height, f.area());

    f.render_widget(Clear, area);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::CYAN));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // 计算可见字段范围（支持滚动）
    let inner_height = inner.height as usize;
    let field_height = 3; // 普通字段占 3 行

    // Provider 对话框: field[0] (Type) 固定显示不参与滚动，只滚动 field[1..]
    let (visible_fields_range,) = if is_provider_dialog {
        let remaining_fields = fields.len().saturating_sub(1);
        let type_height = select_split_height(&fields[0]) as usize;
        let remaining_height = inner_height.saturating_sub(type_height).saturating_sub(2);
        // 计算剩余区域能容纳多少字段行（select 字段占 split_select_height）
        let mut max_vis = 0;
        let mut used = 0usize;
        for fi in 1..fields.len() {
            let h = if fields[fi].is_select() { select_split_height(&fields[fi]) as usize } else { field_height };
            if used + h > remaining_height { break; }
            used += h;
            max_vis += 1;
        }
        let adjusted_focused = focused.saturating_sub(1);
        let offset = if focused == 0 {
            0
        } else if remaining_fields > max_vis {
            if adjusted_focused >= max_vis {
                adjusted_focused.saturating_sub(max_vis / 2).min(remaining_fields.saturating_sub(max_vis))
            } else {
                0
            }
        } else {
            0
        };
        let end = (offset + max_vis).min(remaining_fields);
        (1 + offset..1 + end,)
    } else {
        let max_visible_fields = (inner_height.saturating_sub(2)) / field_height;
        let offset = if fields.len() > max_visible_fields {
            if focused >= max_visible_fields {
                focused.saturating_sub(max_visible_fields / 2).min(fields.len().saturating_sub(max_visible_fields))
            } else {
                0
            }
        } else {
            0
        };
        let end = (offset + max_visible_fields).min(fields.len());
        (offset..end,)
    };

    // 构建 layout constraints
    let mut constraints: Vec<Constraint> = Vec::new();

    if is_provider_dialog {
        constraints.push(Constraint::Length(select_split_height(&fields[0])));
    }

    // 其余可见字段
    let visible_fields = &fields[visible_fields_range.clone()];
    for (i, field) in visible_fields.iter().enumerate() {
        if is_provider_dialog && field.is_select() {
            constraints.push(Constraint::Length(select_split_height(field)));
        } else {
            constraints.push(Constraint::Length(3));
            let actual_index = visible_fields_range.start + i;
            if !is_provider_dialog && actual_index == focused && field.is_select() {
                constraints.push(Constraint::Length(dropdown_rows));
            }
        }
    }
    constraints.push(Constraint::Length(2));
    constraints.push(Constraint::Min(0));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let mut chunk_idx = 0;

    // Provider 对话框: 渲染 Type 左右布局
    if is_provider_dialog {
        render_select_field_split(f, &fields[0], focused == 0, chunks[chunk_idx]);
        chunk_idx += 1;
    }

    // 渲染其余字段
    for (i, field) in visible_fields.iter().enumerate() {
        let actual_index = visible_fields_range.start + i;
        let is_focused = actual_index == focused;
        let border_color = if is_focused { theme::YELLOW } else { theme::MUTED };
        let label_color = if is_focused { theme::YELLOW } else { theme::TEXT };

        // Provider 对话框: select 字段全部用左右布局
        if is_provider_dialog && field.is_select() {
            render_select_field_split(f, field, is_focused, chunks[chunk_idx]);
            chunk_idx += 1;
        } else if field.is_select() {
            let filter = field.select_filter();
            let display = field.display_value();
            let cursor = if is_focused { "▎" } else { "" };
            let value_color = if filter.is_empty() { theme::CYAN } else { theme::TEXT };
            let hint = if is_focused && filter.is_empty() { "  (type to filter, ↑↓ select)" } else { "" };

            let input = Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(format!("{}{}", display, cursor), Style::default().fg(value_color)),
                Span::styled(hint, Style::default().fg(theme::MUTED)),
            ]))
            .block(
                Block::default()
                    .title(Span::styled(format!(" {} ", field.label), Style::default().fg(label_color)))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color))
            );
            f.render_widget(input, chunks[chunk_idx]);
            chunk_idx += 1;

            // 下拉列表（仅聚焦时显示）
            if is_focused {
                if let Some((options, filtered, highlight)) = field.select_state() {
                    // 计算可见窗口
                    let visible_rows = dropdown_rows.saturating_sub(2) as usize; // 减去边框
                    let total_items = filtered.len();

                    // 计算滚动偏移，使高亮项居中
                    let scroll_offset = if highlight >= visible_rows / 2 {
                        (highlight - visible_rows / 2).min(total_items.saturating_sub(visible_rows))
                    } else {
                        0
                    };

                    let visible_end = (scroll_offset + visible_rows).min(total_items);

                    // 只渲染可见范围内的项
                    let items: Vec<ListItem> = filtered[scroll_offset..visible_end]
                        .iter()
                        .enumerate()
                        .map(|(vi, &opt_idx)| {
                            let fi = scroll_offset + vi; // 在 filtered 中的实际索引
                            let name = &options[opt_idx];
                            let is_hl = fi == highlight;
                            let is_current = name == &field.value;
                            let style = if is_hl {
                                Style::default().fg(theme::BASE).bg(theme::CYAN).add_modifier(Modifier::BOLD)
                            } else if is_current {
                                Style::default().fg(theme::GREEN)
                            } else {
                                Style::default().fg(theme::TEXT)
                            };
                            let marker = if is_current { " ● " } else { "   " };
                            ListItem::new(Line::from(vec![
                                Span::styled(marker, style),
                                Span::styled(name.as_str(), style),
                            ]))
                        })
                        .collect();

                    let match_info = format!(" {}/{} ", filtered.len(), options.len());
                    let list = List::new(items)
                        .block(
                            Block::default()
                                .title(Span::styled(match_info, Style::default().fg(theme::MUTED)))
                                .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                                .border_style(Style::default().fg(theme::MUTED))
                        );
                    f.render_widget(list, chunks[chunk_idx]);
                }
                chunk_idx += 1;
            }
        } else {
            // Checkbox 字段
            if field.is_checkbox() {
                let checked = field.value == "true";
                let checkbox_icon = if checked { "[✓]" } else { "[ ]" };
                let cursor = if is_focused { "▎" } else { "" };
                let hint = if is_focused { "  (Space to toggle)" } else { "" };

                let input = Paragraph::new(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(checkbox_icon, Style::default().fg(if checked { theme::GREEN } else { theme::TEXT })),
                    Span::styled(cursor, Style::default().fg(theme::YELLOW)),
                    Span::styled(hint, Style::default().fg(theme::MUTED)),
                ]))
                .block(
                    Block::default()
                        .title(Span::styled(format!(" {} ", field.label), Style::default().fg(label_color)))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(border_color))
                );
                f.render_widget(input, chunks[chunk_idx]);
                chunk_idx += 1;
            } else {
                // 普通文本字段
                let value_color = if field.value.is_empty() { theme::MUTED } else { theme::TEXT };
                let display_value = field.display_value();

            // 密码字段显示切换提示
            let pwd_hint = if field.is_password() && is_focused {
                let icon = if field.is_password_visible() { " 👁 " } else { " *** " };
                vec![Span::styled(format!("  {icon}Ctrl+R"), Style::default().fg(theme::YELLOW))]
            } else if field.label.contains("Supported Models") && is_focused {
                // Supported Models 字段显示快捷键提示
                vec![Span::styled("  (Space view, Ctrl+F fetch, Ctrl+U clear)", Style::default().fg(theme::YELLOW))]
            } else {
                vec![]
            };

            // 构建带光标的文本
            let mut spans = vec![Span::raw(" ")];

            if is_focused {
                // 聚焦时，在光标位置插入光标符号
                let chars: Vec<char> = display_value.chars().collect();
                let cursor_pos = field.cursor_pos.min(chars.len());

                // 计算可见区域（支持水平滚动）
                let available_width = chunks[chunk_idx].width.saturating_sub(4) as usize; // 减去边框和 padding

                // 计算滚动偏移，确保光标可见
                let scroll_offset = if chars.len() > available_width {
                    // 如果光标在右侧不可见区域
                    if cursor_pos >= available_width {
                        cursor_pos.saturating_sub(available_width / 2)
                    } else {
                        0
                    }
                } else {
                    0
                };

                let visible_start = scroll_offset;
                let visible_end = (scroll_offset + available_width).min(chars.len());
                let visible_chars = &chars[visible_start..visible_end];
                let visible_cursor_pos = cursor_pos.saturating_sub(scroll_offset);

                // 显示滚动指示器
                if scroll_offset > 0 {
                    spans.push(Span::styled("◀", Style::default().fg(theme::MUTED)));
                }

                // 光标前的文本
                if visible_cursor_pos > 0 {
                    let before: String = visible_chars[..visible_cursor_pos].iter().collect();
                    spans.push(Span::styled(before, Style::default().fg(value_color)));
                }

                // 光标 - 使用块状样式
                if visible_cursor_pos < visible_chars.len() {
                    // 光标在字符上：反色显示该字符
                    let cursor_char = visible_chars[visible_cursor_pos].to_string();
                    spans.push(Span::styled(
                        cursor_char,
                        Style::default().bg(theme::TEXT).fg(theme::BASE)
                    ));

                    // 光标后的文本
                    if visible_cursor_pos + 1 < visible_chars.len() {
                        let after: String = visible_chars[visible_cursor_pos + 1..].iter().collect();
                        spans.push(Span::styled(after, Style::default().fg(value_color)));
                    }
                } else if cursor_pos == chars.len() {
                    // 光标在末尾：显示空格块
                    spans.push(Span::styled(
                        " ",
                        Style::default().bg(theme::TEXT).fg(theme::BASE)
                    ));
                }

                // 右侧滚动指示器
                if visible_end < chars.len() {
                    spans.push(Span::styled("▶", Style::default().fg(theme::MUTED)));
                }
            } else {
                // 未聚焦时，正常显示（截断长文本）
                let available_width = chunks[chunk_idx].width.saturating_sub(4) as usize;
                if display_value.len() > available_width {
                    let truncated = display_value.chars().take(available_width.saturating_sub(3)).collect::<String>();
                    spans.push(Span::styled(format!("{}...", truncated), Style::default().fg(value_color)));
                } else {
                    spans.push(Span::styled(display_value, Style::default().fg(value_color)));
                }
            }

            spans.extend(pwd_hint);

            let input = Paragraph::new(Line::from(spans))
                .block(
                    Block::default()
                        .title(Span::styled(format!(" {} ", field.label), Style::default().fg(label_color)))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(border_color))
                );
            f.render_widget(input, chunks[chunk_idx]);
            chunk_idx += 1;
            }
        }
    }

    // 按钮行
    if title.contains("Provider") {
        // Provider 对话框显示 Ctrl+F 提示
        let buttons = Paragraph::new(Line::from(vec![
            Span::styled("  [Enter] Save  ", Style::default().fg(theme::GREEN).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled("  [Esc] Cancel  ", Style::default().fg(theme::RED).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled("  [Tab] Next  ", Style::default().fg(theme::TEXT)),
            Span::raw("  "),
            Span::styled("  [^U] Clear  ", Style::default().fg(theme::TEXT)),
            Span::raw("  "),
            Span::styled("  [^K] Copy  ", Style::default().fg(theme::TEXT)),
            Span::raw("  "),
            Span::styled("  [^F] Fetch Models  ", Style::default().fg(theme::CYAN)),
        ]));
        f.render_widget(buttons, chunks[chunk_idx]);
    } else {
        let buttons = Paragraph::new(Line::from(vec![
            Span::styled("  [Enter] Save  ", Style::default().fg(theme::GREEN).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled("  [Esc] Cancel  ", Style::default().fg(theme::RED).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled("  [Tab] Next  ", Style::default().fg(theme::TEXT)),
            Span::raw("  "),
            Span::styled("  [^U] Clear  ", Style::default().fg(theme::TEXT)),
            Span::raw("  "),
            Span::styled("  [^K] Copy  ", Style::default().fg(theme::TEXT)),
        ]));
        f.render_widget(buttons, chunks[chunk_idx]);
    }
}

/// 渲染帮助对话框
pub fn render_help_dialog(f: &mut Frame) {
    let area = centered_rect(70, 75, f.area());

    f.render_widget(Clear, area);

    let help_text = vec![
        Line::from(Span::styled("Global", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  1-5       Switch tabs (Providers/Proxy/Logs/Stats/Settings)", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  q/Ctrl+C  Quit", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  ?/F1      Toggle help", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  r         Refresh", Style::default().fg(theme::TEXT))),
        Line::from(""),
        Line::from(Span::styled("Providers Tab", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  ↑↓/j/k    Navigate list", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  Enter     Activate provider (switch)", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  a         Add new provider", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  e         Edit provider", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  d         Delete provider", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  v         View supported models", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  p         Configure pricing", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  m         Configure model mappings", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  o         Configure auth header", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  r         Reset circuit breaker", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  s         Manage sync settings", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  I         Import providers (from ~/.mrouter/providers.json)", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  E         Export providers (to ~/.mrouter/providers.json)", Style::default().fg(theme::TEXT))),
        Line::from(""),
        Line::from(Span::styled("Proxy Tab", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  s         Start proxy", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  p         Stop proxy", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  e         Edit proxy config", Style::default().fg(theme::TEXT))),
        Line::from(""),
        Line::from(Span::styled("Logs Tab", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  ↑↓        Navigate logs", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  ←→        Previous/Next page", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  Enter     View log detail", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  Esc       Back to list (from detail)", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  r         Refresh logs", Style::default().fg(theme::TEXT))),
        Line::from(""),
        Line::from(Span::styled("Stats Tab", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  t         Toggle time range (Today/Week/Month/All)", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  r         Refresh statistics", Style::default().fg(theme::TEXT))),
        Line::from(""),
        Line::from(Span::styled("Settings Tab", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  ↑↓        Navigate settings", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  e         Edit setting", Style::default().fg(theme::TEXT))),
        Line::from(""),
        Line::from(Span::styled("Input Dialog", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  Enter     Submit", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  Esc       Cancel", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  Tab       Next field", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  Shift+Tab Previous field", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  Ctrl+U    Clear field", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  Ctrl+K    Copy field value", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  Ctrl+V    Paste from clipboard", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  Ctrl+R    Toggle password visibility", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  Ctrl+F    Fetch models from API", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("  Space     Toggle checkbox / Open model viewer", Style::default().fg(theme::TEXT))),
        Line::from(""),
        Line::from(Span::styled("Supported Providers", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  Anthropic, OpenAI, Google AI Studio, AWS Bedrock,", Style::default().fg(theme::SUBTEXT))),
        Line::from(Span::styled("  Azure OpenAI, Vertex AI, Mistral, Cohere, DeepSeek,", Style::default().fg(theme::SUBTEXT))),
        Line::from(Span::styled("  xAI, Meta, MiniMax, Zhipu, Moonshot, Baichuan,", Style::default().fg(theme::SUBTEXT))),
        Line::from(Span::styled("  OpenRouter, Together, Fireworks, Groq, Custom...", Style::default().fg(theme::SUBTEXT))),
        Line::from(""),
        Line::from(Span::styled("Press Esc or ? to close", Style::default().fg(theme::MUTED))),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .title(" Keyboard Shortcuts ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::CYAN))
        )
        .scroll((0, 0));

    f.render_widget(paragraph, area);
}

/// 渲染多选对话框
pub fn render_multiselect_dialog(
    f: &mut Frame,
    title: &str,
    message: &str,
    options: &[String],
    selected: &[usize],
    highlighted: usize,
) {
    let height = (options.len() as u16).min(15) + 8;
    let area = centered_rect_fixed(60, height, f.area());

    f.render_widget(Clear, area);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::CYAN));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(inner);

    // 消息
    let msg = Paragraph::new(message)
        .style(Style::default().fg(theme::SUBTEXT));
    f.render_widget(msg, chunks[0]);

    // 选项列表
    let items: Vec<ListItem> = options.iter().enumerate().map(|(i, opt)| {
        let is_selected = selected.contains(&i);
        let is_highlighted = i == highlighted;

        let checkbox = if is_selected { "[✓]" } else { "[ ]" };
        let style = if is_highlighted {
            Style::default().fg(theme::BASE).bg(theme::CYAN).add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default().fg(theme::GREEN)
        } else {
            Style::default().fg(theme::TEXT)
        };

        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(checkbox, style),
            Span::raw(" "),
            Span::styled(opt.as_str(), style),
        ]))
    }).collect();

    let list = List::new(items);
    f.render_widget(list, chunks[1]);

    // 按钮
    let buttons = Paragraph::new(Line::from(vec![
        Span::styled("  [Space] Toggle  ", Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("  [Enter] Save  ", Style::default().fg(theme::GREEN).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("  [Esc] Cancel  ", Style::default().fg(theme::RED).add_modifier(Modifier::BOLD)),
    ]));
    f.render_widget(buttons, chunks[2]);
}


/// 计算居中矩形（百分比）
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// 计算居中矩形（宽度百分比 + 固定行高）
fn centered_rect_fixed(percent_x: u16, height: u16, r: Rect) -> Rect {
    let h = height.min(r.height.saturating_sub(2));
    let top = r.y + (r.height.saturating_sub(h)) / 2;

    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(r);

    Rect {
        x: horiz[1].x,
        y: top,
        width: horiz[1].width,
        height: h,
    }
}

/// 渲染模型列表浏览窗口
pub fn render_model_list_viewer(
    f: &mut Frame,
    title: &str,
    models: &[String],
    scroll_offset: usize,
) {
    let area = centered_rect_fixed(70, 25, f.area());

    f.render_widget(Clear, area);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::CYAN));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // 标题行
            Constraint::Min(3),     // 模型列表
            Constraint::Length(2),  // 按钮行
        ])
        .split(inner);

    // 标题行
    let info = format!("Total: {} models", models.len());
    let info_line = Paragraph::new(info)
        .style(Style::default().fg(theme::MUTED));
    f.render_widget(info_line, chunks[0]);

    // 模型列表
    let visible_rows = chunks[1].height as usize;
    let total_models = models.len();
    let scroll_offset = scroll_offset.min(total_models.saturating_sub(visible_rows));

    let visible_models: Vec<ListItem> = models
        .iter()
        .skip(scroll_offset)
        .take(visible_rows)
        .enumerate()
        .map(|(i, model)| {
            let index = scroll_offset + i + 1;
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:3}. ", index), Style::default().fg(theme::MUTED)),
                Span::styled(model.as_str(), Style::default().fg(theme::TEXT)),
            ]))
        })
        .collect();

    let scroll_info = if total_models > visible_rows {
        format!(" {}-{}/{} ", scroll_offset + 1, (scroll_offset + visible_rows).min(total_models), total_models)
    } else {
        String::new()
    };

    let list = List::new(visible_models)
        .block(
            Block::default()
                .title(Span::styled(scroll_info, Style::default().fg(theme::YELLOW)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::MUTED))
        );
    f.render_widget(list, chunks[1]);

    // 按钮行
    let buttons = Paragraph::new(Line::from(vec![
        Span::styled("  [↑↓] Scroll  ", Style::default().fg(theme::SUBTEXT)),
        Span::raw("  "),
        Span::styled("  [Esc] Close  ", Style::default().fg(theme::RED).add_modifier(Modifier::BOLD)),
    ]));
    f.render_widget(buttons, chunks[2]);
}
