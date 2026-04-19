use blazar::chat::theme::build_theme;
use blazar::config::load_theme_config_from_path;
use ratatui_core::style::{Color, Style};
use std::sync::{Mutex, MutexGuard};

static THEME_FILE_LOCK: Mutex<()> = Mutex::new(());
const SHARED_THEME_PATH: &str = "config/theme.json";

#[test]
fn theme_config_loads_one_dark_pro_as_the_default_theme() {
    let _guard = lock_theme_file();
    let config = load_theme_config_from_path(SHARED_THEME_PATH).expect("theme config should load");

    assert_eq!(config.active_theme, "one-dark-pro");
    let palette = config
        .themes
        .get("one-dark-pro")
        .expect("one dark pro palette should exist");
    assert_eq!(palette.background, "#282c34");
    assert_eq!(palette.accent, "#61afef");
}

#[test]
fn build_theme_maps_one_dark_pro_styles() {
    let _guard = lock_theme_file();

    let theme = build_theme();

    assert_eq!(theme.shell_border, Style::default().fg(Color::Rgb(97, 175, 239)));
    assert_eq!(theme.rail_border, Style::default().fg(Color::Rgb(86, 182, 194)));
    assert_eq!(
        theme.active_nav,
        Style::default()
            .fg(Color::Rgb(40, 44, 52))
            .bg(Color::Rgb(97, 175, 239))
    );
    assert_eq!(
        theme.user_bubble,
        Style::default()
            .fg(Color::Rgb(40, 44, 52))
            .bg(Color::Rgb(86, 182, 194))
    );
}

fn lock_theme_file() -> MutexGuard<'static, ()> {
    THEME_FILE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
