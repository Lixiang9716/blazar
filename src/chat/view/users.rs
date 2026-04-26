use crate::chat::app::ChatApp;
use crate::chat::theme::ChatTheme;
use ratatui_core::{layout::Rect, terminal::Frame};
use ratatui_macros::vertical;

use super::{input, status};

pub(super) fn render_users_area(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let [status_area, users_tail] = vertical![==1, >=0].areas(area);
    let [input_area, mode_area] = vertical![>=0, ==1].areas(users_tail);
    status::render_users_status_row(frame, status_area, app, theme);
    input::render_input(frame, input_area, app, theme);
    status::render_mode_config_row(frame, mode_area, app, theme);
}
