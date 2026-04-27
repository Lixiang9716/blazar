use crate::chat::{app::ChatApp, theme::ChatTheme, view::input};
use ratatui_core::{layout::Rect, terminal::Frame};

pub(in crate::chat::view) fn render_input_panel(
    frame: &mut Frame,
    area: Rect,
    app: &ChatApp,
    theme: &ChatTheme,
) {
    input::render_input(frame, area, app, theme);
}
