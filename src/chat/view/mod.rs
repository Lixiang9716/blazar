//! Chat rendering — each sub-module handles one visual region.

mod banner;
mod input;
mod picker;
mod status;
mod streaming;
mod timeline;

use crate::chat::app::ChatApp;
use core::cmp;
use ratatui_core::{
    backend::TestBackend,
    terminal::{Frame, Terminal},
};
use ratatui_macros::vertical;
use ratatui_widgets::block::Block;
use unicode_width::UnicodeWidthStr;

pub fn render_to_lines_for_test(app: &mut ChatApp, width: u16, height: u16) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

    terminal
        .draw(|frame| render_frame(frame, app, 1_200))
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
    let users_height = users_height(area.height);
    let [timeline_zone, users_area] = vertical![>=1, ==(users_height)].areas(area);
    let streaming_height: u16 = if streaming { 1 } else { 0 };
    let banner_height = if app.has_user_sent() {
        0
    } else {
        12.min(timeline_zone.height.saturating_sub(1 + streaming_height))
    };

    let [banner_area, timeline_area, streaming_area] =
        vertical![==(banner_height), >=1, ==(streaming_height)].areas(timeline_zone);

    if banner_height > 0 {
        banner::render_welcome_banner(frame, banner_area, app, tick_ms, &theme);
    }
    timeline::render_timeline(frame, timeline_area, app, &theme);

    if streaming {
        streaming::render_streaming_indicator(frame, streaming_area, tick_ms, app, &theme);
    }

    let [status_area, users_tail] = vertical![==1, >=0].areas(users_area);
    let [input_area, mode_area] = vertical![>=0, ==1].areas(users_tail);
    status::render_users_status_row(frame, status_area, app, &theme);
    input::render_input(frame, input_area, app, &theme);
    status::render_mode_config_row(frame, mode_area, app, &theme);

    if app.picker.is_visible() {
        picker::render_picker(frame, area, app, &theme);
    }
}

fn users_height(total_height: u16) -> u16 {
    if total_height <= 1 {
        0
    } else {
        3.min(total_height.saturating_sub(1))
    }
}
