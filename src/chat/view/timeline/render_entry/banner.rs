use super::*;
use crate::welcome::mascot::render_mascot_lines;
use crate::welcome::state::WelcomeState;
use std::sync::OnceLock;
use std::time::Instant;
use unicode_width::UnicodeWidthStr;

static BANNER_EPOCH: OnceLock<Instant> = OnceLock::new();

/// Render the welcome banner inline in the timeline.
///
/// Layout: colored solid border wrapping [mascot | text] side-by-side.
pub(super) fn render_banner_entry<'a>(
    theme: &ChatTheme,
    _width: u16,
    workspace: &str,
    branch: &str,
) -> Vec<Line<'a>> {
    let version = env!("CARGO_PKG_VERSION");
    let border_style = theme.marker_tool;

    let epoch = BANNER_EPOCH.get_or_init(Instant::now);
    let tick_ms = epoch.elapsed().as_millis() as u64;

    // Build mascot rows (preserve styles)
    let mascot_raw = render_mascot_lines(WelcomeState::new(), tick_ms);
    let mascot_rows: Vec<Vec<Span<'static>>> = mascot_raw
        .into_iter()
        .map(|line| {
            line.spans
                .into_iter()
                .map(|s| Span::styled(s.content.into_owned(), s.style))
                .collect()
        })
        .collect();
    let mascot_height = mascot_rows.len();
    let mascot_width = mascot_rows
        .iter()
        .map(|spans| spans.iter().map(|s| s.content.width()).sum::<usize>())
        .max()
        .unwrap_or(0);

    // Build text rows
    let gap = 2usize;
    let mut text_rows: Vec<Vec<Span<'a>>> = Vec::new();

    // Title
    text_rows.push(vec![
        Span::styled("Blazar", theme.title_text),
        Span::styled(format!(" v{version}"), theme.dim_text),
    ]);

    // Workspace + branch
    if !workspace.is_empty() {
        let mut ctx = vec![Span::styled(workspace.to_owned(), theme.bold_text)];
        if !branch.is_empty() {
            ctx.push(Span::styled(" on ", theme.dim_text));
            ctx.push(Span::styled(format!(" {branch}"), theme.marker_tool));
        }
        text_rows.push(ctx);
    }

    // Prompt
    text_rows.push(vec![Span::styled(
        "Describe a task to get started.",
        theme.body_text,
    )]);

    // Blank separator
    text_rows.push(vec![]);

    // Tips
    text_rows.push(vec![
        Span::styled("Tip: ", theme.dim_text),
        Span::styled("/help", theme.tip_command),
        Span::styled(" for commands, ", theme.dim_text),
        Span::styled("Ctrl+O", theme.tip_command),
        Span::styled(" toggle details.", theme.dim_text),
    ]);
    text_rows.push(vec![Span::styled(
        "Blazar uses AI. Check for mistakes.",
        theme.dim_text,
    )]);

    let content_height = std::cmp::max(mascot_height, text_rows.len());

    // Measure the widest text row
    let text_max_width = text_rows
        .iter()
        .map(|spans| spans.iter().map(|s| s.content.width()).sum::<usize>())
        .max()
        .unwrap_or(0);

    // Inner content: pad(1) + mascot + gap + text + pad(1)
    let inner_width = 1 + mascot_width + gap + text_max_width + 1;

    // Vertically center text within content_height
    let text_y_offset = content_height.saturating_sub(text_rows.len()) / 2;

    // Build combined lines
    let mut lines: Vec<Line<'a>> = Vec::new();

    // Top border: ╭─────╮
    lines.push(Line::from(vec![
        Span::raw(MARGIN),
        Span::styled("╭", border_style),
        Span::styled("─".repeat(inner_width), border_style),
        Span::styled("╮", border_style),
    ]));

    for row_idx in 0..content_height {
        let mut spans: Vec<Span<'a>> = Vec::new();
        spans.push(Span::raw(MARGIN));
        spans.push(Span::styled("│ ", border_style));

        // Mascot column
        if row_idx < mascot_rows.len() {
            let row_spans = &mascot_rows[row_idx];
            let row_w: usize = row_spans.iter().map(|s| s.content.width()).sum();
            for s in row_spans {
                spans.push(Span::styled(s.content.to_string(), s.style));
            }
            if row_w < mascot_width {
                spans.push(Span::raw(" ".repeat(mascot_width - row_w)));
            }
        } else {
            spans.push(Span::raw(" ".repeat(mascot_width)));
        }

        // Gap
        spans.push(Span::raw(" ".repeat(gap)));

        // Text column (pad to text_max_width)
        let text_idx = row_idx.checked_sub(text_y_offset);
        let text_w = if let Some(ti) = text_idx {
            if ti < text_rows.len() {
                let w: usize = text_rows[ti].iter().map(|s| s.content.width()).sum();
                for s in &text_rows[ti] {
                    spans.push(Span::styled(s.content.to_string(), s.style));
                }
                w
            } else {
                0
            }
        } else {
            0
        };
        if text_w < text_max_width {
            spans.push(Span::raw(" ".repeat(text_max_width - text_w)));
        }

        spans.push(Span::styled(" │", border_style));
        lines.push(Line::from(spans));
    }

    // Bottom border: ╰─────╯
    lines.push(Line::from(vec![
        Span::raw(MARGIN),
        Span::styled("╰", border_style),
        Span::styled("─".repeat(inner_width), border_style),
        Span::styled("╯", border_style),
    ]));

    lines
}
