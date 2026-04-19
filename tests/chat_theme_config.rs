use blazar::chat::theme::build_theme;
use blazar::config::{ConfigError, load_theme_config_from_path};
use ratatui_core::style::{Color, Style};
use std::fs;
use std::path::{Path, PathBuf};
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
fn theme_config_rejects_an_unknown_active_theme() {
    let path = write_test_config(
        "unknown-active-theme.json",
        r##"{
  "activeTheme": "missing-theme",
  "themes": {
    "one-dark-pro": {
      "background": "#282c34",
      "surface": "#21252b",
      "text": "#abb2bf",
      "muted": "#5c6370",
      "accent": "#61afef",
      "success": "#98c379",
      "warning": "#e5c07b",
      "danger": "#e06c75",
      "spirit": "#c678dd",
      "info": "#56b6c2"
    }
  },
  "density": "comfortable",
  "uppercaseLabels": false
}"##,
    );

    let error = load_theme_config_from_path(&path).expect_err("unknown theme should fail");

    assert_eq!(
        error.to_string(),
        format!(
            "invalid config schema {}: activeTheme must reference a theme in themes",
            path.display()
        )
    );

    cleanup_test_config(&path);
}

#[test]
fn theme_config_rejects_invalid_hex_colors() {
    let path = write_test_config(
        "invalid-hex-theme.json",
        r##"{
  "activeTheme": "one-dark-pro",
  "themes": {
    "one-dark-pro": {
      "background": "#282c34",
      "surface": "#21252b",
      "text": "#abb2bf",
      "muted": "#5c6370",
      "accent": "blue",
      "success": "#98c379",
      "warning": "#e5c07b",
      "danger": "#e06c75",
      "spirit": "#c678dd",
      "info": "#56b6c2"
    }
  },
  "density": "comfortable",
  "uppercaseLabels": false
}"##,
    );

    let error = load_theme_config_from_path(&path).expect_err("invalid hex should fail");

    assert_eq!(
        error.to_string(),
        format!(
            "invalid config schema {}: theme colors must use #RRGGBB",
            path.display()
        )
    );

    cleanup_test_config(&path);
}

#[test]
fn theme_config_returns_read_errors_for_missing_files() {
    let path = PathBuf::from("target/test-artifacts/missing-theme.json");
    cleanup_test_config(&path);

    let error = load_theme_config_from_path(&path).expect_err("missing theme file should fail");

    assert!(matches!(error, ConfigError::Read { path: error_path, .. } if error_path == path));
}

#[test]
fn theme_config_returns_parse_errors_for_malformed_json() {
    let path = write_test_config("malformed-theme.json", "{ not-valid-json }");

    let error = load_theme_config_from_path(&path).expect_err("malformed theme should fail");

    assert!(matches!(error, ConfigError::Parse { path: error_path, .. } if error_path == path));

    cleanup_test_config(&path);
}

#[test]
fn build_theme_maps_one_dark_pro_styles_and_caches_the_result() {
    let _guard = lock_theme_file();
    let original = fs::read_to_string(SHARED_THEME_PATH).expect("theme config should exist");
    let _restore = ThemeFileRestore::new(SHARED_THEME_PATH, original);

    let theme = build_theme();

    assert_eq!(
        theme.shell_border,
        Style::default().fg(Color::Rgb(97, 175, 239))
    );
    assert_eq!(
        theme.rail_border,
        Style::default().fg(Color::Rgb(86, 182, 194))
    );
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

    fs::write(SHARED_THEME_PATH, "{ not-valid-json }").expect("theme file should be overwritten");

    let cached = build_theme();
    assert_eq!(cached.shell_border, theme.shell_border);
    assert_eq!(cached.user_bubble, theme.user_bubble);
}

fn lock_theme_file() -> MutexGuard<'static, ()> {
    THEME_FILE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_test_config(file_name: &str, contents: &str) -> PathBuf {
    let dir = Path::new("target/test-artifacts");
    fs::create_dir_all(dir).expect("test artifact directory should exist");
    let path = dir.join(file_name);
    fs::write(&path, contents).expect("test config should be written");
    path
}

fn cleanup_test_config(path: &Path) {
    if path.exists() {
        fs::remove_file(path).expect("test config should be removed");
    }
}

struct ThemeFileRestore {
    path: &'static str,
    original: String,
}

impl ThemeFileRestore {
    fn new(path: &'static str, original: String) -> Self {
        Self { path, original }
    }
}

impl Drop for ThemeFileRestore {
    fn drop(&mut self) {
        fs::write(self.path, &self.original).expect("theme config should be restored");
    }
}
