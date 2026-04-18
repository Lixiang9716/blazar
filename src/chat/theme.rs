use ratatui::style::{Color, Style};

pub struct ChatTheme {
    pub spirit_border: Style,
    pub chat_border: Style,
    pub spirit_bubble: Style,
    pub user_bubble: Style,
}

pub fn build_theme() -> ChatTheme {
    ChatTheme {
        spirit_border: Style::default().fg(Color::Cyan),
        chat_border: Style::default().fg(Color::Blue),
        spirit_bubble: Style::default().fg(Color::White).bg(Color::Rgb(70, 40, 90)),
        user_bubble: Style::default()
            .fg(Color::Black)
            .bg(Color::Rgb(120, 210, 255)),
    }
}
