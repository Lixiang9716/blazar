use ratatui_core::style::{Color, Modifier, Style};

// Solarized Dark palette
pub const BASE03: Color = Color::Rgb(0, 43, 54);
pub const BASE02: Color = Color::Rgb(7, 54, 66);
pub const BASE01: Color = Color::Rgb(88, 110, 117);
pub const BASE00: Color = Color::Rgb(101, 123, 131);
pub const BASE0: Color = Color::Rgb(131, 148, 150);
pub const BASE1: Color = Color::Rgb(147, 161, 161);
pub const BASE2: Color = Color::Rgb(238, 232, 213);
pub const BASE3: Color = Color::Rgb(253, 246, 227);

pub const YELLOW: Color = Color::Rgb(181, 137, 0);
pub const ORANGE: Color = Color::Rgb(203, 75, 22);
pub const RED: Color = Color::Rgb(220, 50, 47);
pub const MAGENTA: Color = Color::Rgb(211, 54, 130);
pub const VIOLET: Color = Color::Rgb(108, 113, 196);
pub const BLUE: Color = Color::Rgb(38, 139, 210);
pub const CYAN: Color = Color::Rgb(42, 161, 152);
pub const GREEN: Color = Color::Rgb(133, 153, 0);

pub struct ChatTheme {
    pub title_bar: Style,
    pub title_text: Style,
    pub timeline_bg: Style,
    pub body_text: Style,
    pub dim_text: Style,
    pub bold_text: Style,
    pub marker_response: Style,
    pub marker_tool: Style,
    pub marker_bash: Style,
    pub marker_thinking: Style,
    pub tool_label: Style,
    pub tool_target: Style,
    pub diff_add: Style,
    pub diff_del: Style,
    pub code_block: Style,
    pub input_prompt: Style,
    pub input_placeholder: Style,
    pub status_bar: Style,
    pub status_right: Style,
}

pub fn build_theme() -> ChatTheme {
    ChatTheme {
        title_bar: Style::default().fg(BASE1),
        title_text: Style::default().fg(BASE2).add_modifier(Modifier::BOLD),
        timeline_bg: Style::default().fg(BASE0),
        body_text: Style::default().fg(BASE0),
        dim_text: Style::default().fg(BASE01),
        bold_text: Style::default().fg(BASE1).add_modifier(Modifier::BOLD),
        marker_response: Style::default().fg(YELLOW),
        marker_tool: Style::default().fg(GREEN),
        marker_bash: Style::default().fg(GREEN),
        marker_thinking: Style::default().fg(YELLOW),
        tool_label: Style::default().fg(BASE2).add_modifier(Modifier::BOLD),
        tool_target: Style::default().fg(CYAN),
        diff_add: Style::default().fg(GREEN),
        diff_del: Style::default().fg(RED),
        code_block: Style::default().fg(BASE0),
        input_prompt: Style::default().fg(CYAN),
        input_placeholder: Style::default().fg(BASE01),
        status_bar: Style::default().fg(BASE1),
        status_right: Style::default().fg(BASE01),
    }
}
