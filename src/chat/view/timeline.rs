use crate::chat::app::ChatApp;
use crate::chat::model::{Actor, EntryKind, TimelineEntry, ToolCallStatus};
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

use render_entry::render_entry;

#[cfg(test)]
use markdown::{MdSegment, split_code_fences};
#[cfg(test)]
use render_entry::render_fenced_code;

/// Left margin for all timeline content (matches Claude Code's indentation).
const MARGIN: &str = "  ";
/// Continuation indent (margin + marker width).
const INDENT: &str = "    ";
const INDENT_WIDTH: u16 = 4;

pub(super) fn render_timeline(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let mut lines: Vec<Line> = Vec::new();
    let show_details = app.show_details();

    let content_width = area.width;

    // Track logical turns for turn headers
    let mut last_actor: Option<Actor> = None;
    let mut user_turn = 0u16;
    let mut assistant_turn = 0u16;

    for entry in app.timeline() {
        // Insert turn header when actor changes between User and Assistant
        let is_user = entry.actor == Actor::User && entry.kind == EntryKind::Message;
        let is_assistant = entry.actor == Actor::Assistant && entry.kind == EntryKind::Message;

        if is_user {
            if last_actor != Some(Actor::User) {
                user_turn += 1;
                if !lines.is_empty() {
                    // Subtle separator between turns
                    let sep = "─".repeat(content_width.saturating_sub(4) as usize);
                    lines.push(Line::from(vec![
                        Span::raw(MARGIN),
                        Span::styled(sep, theme.dim_text),
                    ]));
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(vec![
                    Span::raw(MARGIN),
                    Span::styled(format!("You #{user_turn}"), theme.bold_text),
                ]));
            }
            last_actor = Some(Actor::User);
        } else if is_assistant {
            if last_actor != Some(Actor::Assistant) {
                assistant_turn += 1;
                if !lines.is_empty() {
                    let sep = "─".repeat(content_width.saturating_sub(4) as usize);
                    lines.push(Line::from(vec![
                        Span::raw(MARGIN),
                        Span::styled(sep, theme.dim_text),
                    ]));
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(vec![
                    Span::raw(MARGIN),
                    Span::styled(
                        entry
                            .title
                            .clone()
                            .unwrap_or_else(|| format!("Blazar #{assistant_turn}")),
                        theme.marker_response,
                    ),
                ]));
            }
            last_actor = Some(Actor::Assistant);
        }
        // Tool/thinking/etc entries stay within the current assistant turn

        let entry_lines = render_entry(entry, theme, content_width);
        lines.extend(entry_lines);

        // Show expanded details when Ctrl+O is toggled
        if show_details && !entry.details.is_empty() {
            lines.push(Line::from(""));
            for detail_line in entry.details.lines() {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(detail_line.to_owned(), theme.dim_text),
                ]));
            }
        }

        lines.push(Line::from("")); // blank separator
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
