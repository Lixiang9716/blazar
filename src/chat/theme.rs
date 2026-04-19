use ratatui_core::style::{Color, Style};

pub struct ChatTheme {
    pub shell_border: Style,
    pub rail_border: Style,
    pub panel_border: Style,
    pub active_nav: Style,
    pub inactive_nav: Style,
    pub status_text: Style,
    pub spirit_border: Style,
    pub chat_border: Style,
    pub spirit_bubble: Style,
    pub user_bubble: Style,
}

pub fn build_theme() -> ChatTheme {
    ChatTheme {
        shell_border: Style::default().fg(Color::Cyan),
        rail_border: Style::default().fg(Color::Blue),
        panel_border: Style::default().fg(Color::Gray),
        active_nav: Style::default().fg(Color::White).bg(Color::Blue),
        inactive_nav: Style::default().fg(Color::Gray),
        status_text: Style::default().fg(Color::Magenta),
        spirit_border: Style::default().fg(Color::Cyan),
        chat_border: Style::default().fg(Color::Blue),
        spirit_bubble: Style::default().fg(Color::White).bg(Color::Rgb(70, 40, 90)),
        user_bubble: Style::default()
            .fg(Color::Black)
            .bg(Color::Rgb(120, 210, 255)),
    }
}
