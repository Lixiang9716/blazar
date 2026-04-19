//! Chat rendering — each sub-module handles one visual region.

mod banner;
mod input;
mod picker;
mod status;
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

    // Clear pending effects so animations don't alter the test buffer.
    app.effects = Default::default();

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
    let elapsed = app.elapsed_since_last_frame();
    let theme = app.theme().clone();
    let area = frame.area();

    let bg_block = Block::default().style(theme.timeline_bg);
    frame.render_widget(bg_block, area);

    let [
        banner_area,
        timeline_area,
        sep_top,
        input_area,
        sep_bot,
        status_area,
    ] = vertical![==12, >=1, ==1, ==1, ==1, ==1].areas(area);

    banner::render_welcome_banner(frame, banner_area, app, tick_ms, &theme);
    timeline::render_timeline(frame, timeline_area, app, &theme);
    status::render_separator(frame, sep_top, &theme);
    input::render_input(frame, input_area, app, &theme);
    status::render_separator(frame, sep_bot, &theme);
    status::render_status_bar(frame, status_area, app, &theme);

    if app.picker.is_visible() {
        picker::render_picker(frame, area, app, &theme);
    }

    // Apply tachyonfx animations after all widgets are rendered.
    app.effects.process(elapsed, frame.buffer_mut(), area);
}
