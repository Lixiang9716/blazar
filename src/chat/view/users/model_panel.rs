use crate::chat::{app::ChatApp, theme::ChatTheme, view::status};
use ratatui_core::{layout::Rect, terminal::Frame};

pub(in crate::chat::view) fn render_model_panel(
    frame: &mut Frame,
    area: Rect,
    app: &ChatApp,
    theme: &ChatTheme,
) {
    status::render_mode_config_row(frame, area, app, theme);
}
