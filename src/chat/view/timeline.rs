use crate::chat::app::ChatApp;
use crate::chat::model::{Actor, EntryKind, TimelineEntry};
use crate::chat::theme::ChatTheme;
use core::cmp;
use ratatui_core::{
    layout::Rect,
    style::{Color, Style},
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::paragraph::{Paragraph, Wrap};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

mod markdown;
mod render_entry;
mod text_wrap;

#[cfg(test)]
#[path = "../../../tests/unit/chat/view/timeline/tests.rs"]
mod tests;

use render_entry::{EntryRenderRegistry, TimelineEntryRenderer};
use text_wrap::push_wrapped_lines;

#[cfg(test)]
use markdown::{MdSegment, split_code_fences};
#[cfg(test)]
use render_entry::render_fenced_code;

/// Left margin for all timeline content (matches Claude Code's indentation).
const MARGIN: &str = "  ";
/// Continuation indent (margin + marker width).
const INDENT: &str = "    ";
const INDENT_WIDTH: u16 = 4;

pub(in crate::chat::view) fn render_timeline(
    frame: &mut Frame,
    area: Rect,
    app: &ChatApp,
    theme: &ChatTheme,
) {
    let renderer = EntryRenderRegistry::default();
    render_timeline_with_renderer(frame, area, app, theme, &renderer);
}

fn render_timeline_with_renderer(
    frame: &mut Frame,
    area: Rect,
    app: &ChatApp,
    theme: &ChatTheme,
    renderer: &dyn TimelineEntryRenderer,
) {
    let mut lines: Vec<Line> = Vec::new();
    let show_details = app.show_details();

    let content_width = area.width;

    for entry in app.timeline() {
        let hide_empty_thinking = entry.kind == EntryKind::Thinking && entry.body.trim().is_empty();
        if hide_empty_thinking {
            continue;
        }

        let entry_lines = renderer.render(entry, theme, content_width);
        lines.extend(entry_lines);

        // Show expanded details when Ctrl+O is toggled
        if show_details && !entry.details.is_empty() {
            lines.push(Line::from(""));
            let detail_lines = render_entry::render_markdown_details_block(
                &entry.details,
                theme,
                content_width,
                vec![Span::raw(INDENT)],
            );
            lines.extend(detail_lines);
        }

        lines.push(Line::from("")); // blank separator
    }

    for queued_text in app.queued_user_texts_for_render() {
        push_wrapped_lines(
            &mut lines,
            &format!("{queued_text} (pending)"),
            theme.bold_text,
            vec![Span::raw(MARGIN), Span::styled("› ", theme.marker_response)],
            content_width,
        );
        lines.push(Line::from(""));
    }

    // If no entries, show welcome
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Welcome to Blazar. Type a message to begin.",
            theme.dim_text,
        )));
    }

    let paragraph = Paragraph::new(lines.clone())
        .style(theme.timeline_bg)
        .wrap(Wrap { trim: false });

    // Compute actual visual height accounting for line wrapping.
    let content_height: u16 = if content_width > 0 {
        lines
            .iter()
            .map(|line| {
                let w = line.width() as u16;
                if w == 0 { 1 } else { w.div_ceil(content_width) }
            })
            .sum()
    } else {
        lines.len() as u16
    };
    let visible_height = area.height;

    // Feed back heights so scroll sentinel can be resolved
    app.timeline_content_height.set(content_height);
    app.timeline_visible_height.set(visible_height);

    let scroll_offset = if content_height > visible_height {
        let auto_scroll = content_height.saturating_sub(visible_height);
        // Respect manual scroll if set
        cmp::min(app.scroll_offset(), auto_scroll)
    } else {
        0
    };

    let paragraph = paragraph.scroll((scroll_offset, 0));

    frame.render_widget(paragraph, area);
}
