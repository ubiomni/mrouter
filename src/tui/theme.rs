// 全局 TUI 主题 — 使用 RGB 颜色确保跨终端一致性
//
// 基于 Doom One 色板（同 Alacritty 配置），在 VS Code、iTerm2、
// Windows Terminal、Alacritty 等现代终端中表现一致。
// 如果终端不支持 true color，crossterm 会自动降级到最接近的 256 色。

use ratatui::style::Color;

// ── 基础色 ──────────────────────────────────────────
/// 背景色（用于需要显式背景的场景）
pub const BASE: Color = Color::Rgb(40, 44, 52);          // #282c34  primary.background
/// 选中行/高亮背景色
pub const HIGHLIGHT_BG: Color = Color::Rgb(55, 60, 72);  // #373c48

// ── 文本色 ──────────────────────────────────────────
/// 主要文本色
pub const TEXT: Color = Color::Rgb(187, 194, 207);        // #bbc2cf  primary.foreground
/// 次要文本色
pub const SUBTEXT: Color = Color::Rgb(155, 162, 176);     // #9ba2b0
/// 弱化文本色（占位符、禁用项）
pub const MUTED: Color = Color::Rgb(91, 98, 104);         // #5b6268  bright.black

// ── 边框色 ──────────────────────────────────────────
/// 默认边框色
pub const BORDER: Color = Color::Rgb(55, 60, 72);         // #373c48

// ── 强调色（取自 bright 色组，更鲜亮）────────────────
pub const CYAN: Color = Color::Rgb(70, 217, 255);         // #46d9ff  bright.cyan
pub const BLUE: Color = Color::Rgb(81, 175, 239);         // #51afef  normal.blue
pub const GREEN: Color = Color::Rgb(152, 190, 101);       // #98be65  normal.green
pub const YELLOW: Color = Color::Rgb(236, 190, 123);      // #ecbe7b  bright.yellow
pub const RED: Color = Color::Rgb(255, 108, 107);         // #ff6c6b  normal.red
pub const MAGENTA: Color = Color::Rgb(198, 120, 221);     // #c678dd  normal.magenta
