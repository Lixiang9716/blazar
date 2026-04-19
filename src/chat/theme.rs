use ratatui_core::style::{Color, Style};

#[derive(Debug, Clone, PartialEq, Eq)]
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

fn parse_hex_rgb(hex: &str) -> Color {
    let original = hex;
    let hex = hex.trim_start_matches('#');
    assert_eq!(hex.len(), 6, "expected #RRGGBB hex color, got {original:?}");
    let r = u8::from_str_radix(&hex[0..2], 16).expect("valid red");
    let g = u8::from_str_radix(&hex[2..4], 16).expect("valid green");
    let b = u8::from_str_radix(&hex[4..6], 16).expect("valid blue");
    Color::Rgb(r, g, b)
}

pub fn build_theme() -> ChatTheme {
    let config = crate::config::load_theme_config().expect("theme config should load");
    let palette = config
        .themes
        .get(&config.active_theme)
        .expect("active theme must exist");

    ChatTheme {
        shell_border: Style::default().fg(parse_hex_rgb(&palette.accent)),
        rail_border: Style::default().fg(parse_hex_rgb(&palette.info)),
        panel_border: Style::default().fg(parse_hex_rgb(&palette.muted)),
        active_nav: Style::default()
            .fg(parse_hex_rgb(&palette.background))
            .bg(parse_hex_rgb(&palette.accent)),
        inactive_nav: Style::default().fg(parse_hex_rgb(&palette.text)),
        status_text: Style::default().fg(parse_hex_rgb(&palette.spirit)),
        spirit_border: Style::default().fg(parse_hex_rgb(&palette.spirit)),
        chat_border: Style::default().fg(parse_hex_rgb(&palette.accent)),
        spirit_bubble: Style::default()
            .fg(parse_hex_rgb(&palette.text))
            .bg(parse_hex_rgb(&palette.surface)),
        user_bubble: Style::default()
            .fg(parse_hex_rgb(&palette.background))
            .bg(parse_hex_rgb(&palette.info)),
    }
}
