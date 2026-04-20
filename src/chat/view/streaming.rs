use crate::chat::theme::ChatTheme;
use crate::welcome::mascot::render_slime_run_lines;
use core::cmp;
use ratatui_core::{
    layout::Rect,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_macros::horizontal;
use ratatui_widgets::paragraph::Paragraph;

/// Render the streaming indicator: slime_run animation on the left,
/// "streaming…" label on the right. Only called when the agent is streaming.
pub(super) fn render_streaming_indicator(
    frame: &mut Frame,
    area: Rect,
    tick_ms: u64,
    theme: &ChatTheme,
) {
    if area.height == 0 || area.width < 4 {
        return;
    }

    let slime_lines = render_slime_run_lines(tick_ms);

    // Determine sprite width from the first non-empty line.
    let sprite_width = slime_lines.first().map(|l| l.width() as u16).unwrap_or(0);
    let col_width = cmp::min(sprite_width + 1, area.width / 3);

    let [sprite_area, label_area] = horizontal![==(col_width), >=1].areas(area);

    // Render sprite — take only the rows that fit, bottom-aligned so
    // the slime feet sit on the separator line.
    let visible_rows = area.height as usize;
    let total_rows = slime_lines.len();
    let skip = total_rows.saturating_sub(visible_rows);
    let cropped: Vec<Line<'static>> = slime_lines.into_iter().skip(skip).collect();

    let sprite_paragraph = Paragraph::new(cropped);
    frame.render_widget(sprite_paragraph, sprite_area);

    // Render "streaming…" label vertically centered in the label area.
    let label = Line::from(vec![
        Span::styled("  streaming", theme.spinner),
        Span::styled("…", theme.dim_text),
    ]);
    let y_center = label_area.height / 2;
    let label_rect = Rect::new(label_area.x, label_area.y + y_center, label_area.width, 1);
    let label_paragraph = Paragraph::new(label);
    frame.render_widget(label_paragraph, label_rect);
}
