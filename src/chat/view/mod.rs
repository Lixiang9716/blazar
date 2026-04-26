//! Chat rendering — each sub-module handles one visual region.

mod input;
mod picker;
mod status;
mod streaming;
mod timeline;
mod users;

use crate::chat::app::ChatApp;
use crate::chat::users_state::UsersLayoutPolicy;
use core::cmp;
use ratatui_core::{
    backend::TestBackend,
    layout::Rect,
    terminal::{Frame, Terminal},
};
use ratatui_macros::vertical;
use ratatui_widgets::block::Block;
use unicode_width::UnicodeWidthStr;

pub fn render_to_lines_for_test(app: &mut ChatApp, width: u16, height: u16) -> Vec<String> {
    render_to_lines_for_test_with_users_policy(app, width, height, UsersLayoutPolicy::default())
}

pub fn render_to_lines_for_test_with_users_policy(
    app: &mut ChatApp,
    width: u16,
    height: u16,
    users_policy: UsersLayoutPolicy,
) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

    terminal
        .draw(|frame| render_frame_with_users_policy(frame, app, 1_200, users_policy))
        .expect("chat frame should render");

    let buffer = terminal.backend().buffer();
    buffer
        .content()
        .chunks(width as usize)
        .map(|row| {
            let mut line = String::new();
            let mut skip = 0;

            for cell in row {
                if skip == 0 {
                    line.push_str(cell.symbol());
                }
                skip = cmp::max(skip, cell.symbol().width()).saturating_sub(1);
            }

            line
        })
        .collect()
}

pub fn render_frame(frame: &mut Frame, app: &mut ChatApp, tick_ms: u64) {
    let theme = app.theme().clone();
    let area = frame.area();

    let bg_block = Block::default().style(theme.timeline_bg);
    frame.render_widget(bg_block, area);

    let streaming = app.is_streaming();
    let users_policy = UsersLayoutPolicy::default();
    let users_height = users::users_area_height(area.height, users_policy);
    let [timeline_area, users_area] = vertical![>=1, ==(users_height)].areas(area);
    timeline::render_timeline(frame, timeline_area, app, &theme);

    if streaming && timeline_area.height > 0 {
        let indicator_area = Rect::new(
            timeline_area.x,
            timeline_area
                .y
                .saturating_add(timeline_area.height.saturating_sub(1)),
            timeline_area.width,
            1,
        );
        streaming::render_streaming_indicator(frame, indicator_area, tick_ms, app, &theme);
    }

    users::render_users_area(frame, users_area, app, &theme);

    if app.picker.is_visible() {
        picker::render_picker(frame, area, app, &theme);
    }
}

pub fn render_frame_with_users_policy(
    frame: &mut Frame,
    app: &mut ChatApp,
    tick_ms: u64,
    users_policy: UsersLayoutPolicy,
) {
    let theme = app.theme().clone();
    let area = frame.area();

    let bg_block = Block::default().style(theme.timeline_bg);
    frame.render_widget(bg_block, area);

    let streaming = app.is_streaming();
    let users_height = users::users_area_height(area.height, users_policy);
    let [timeline_area, users_area] = vertical![>=1, ==(users_height)].areas(area);
    timeline::render_timeline(frame, timeline_area, app, &theme);

    if streaming && timeline_area.height > 0 {
        let indicator_area = Rect::new(
            timeline_area.x,
            timeline_area
                .y
                .saturating_add(timeline_area.height.saturating_sub(1)),
            timeline_area.width,
            1,
        );
        streaming::render_streaming_indicator(frame, indicator_area, tick_ms, app, &theme);
    }

    users::render_users_area_with_policy(frame, users_area, app, &theme, users_policy);

    if app.picker.is_visible() {
        picker::render_picker(frame, area, app, &theme);
    }
}
