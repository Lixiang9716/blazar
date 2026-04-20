use crate::chat::app::ChatApp;
use crate::chat::model::{Actor, EntryKind, TimelineEntry};
use crate::chat::theme::{ChatTheme, ThemeStyleSheet};
use core::cmp;
use ratatui_core::{
    layout::Rect,
    style::Style,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::paragraph::{Paragraph, Wrap};

/// Left margin for all timeline content (matches Claude Code's indentation).
const MARGIN: &str = "  ";
/// Continuation indent (margin + marker width).
const INDENT: &str = "    ";

pub(super) fn render_timeline(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let mut lines: Vec<Line> = Vec::new();
    let show_details = app.show_details();

    for entry in app.timeline() {
        let entry_lines = render_entry(entry, theme, area.width);
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
    let content_height: u16 = if area.width > 0 {
        lines
            .iter()
            .map(|line| {
                let w = line.width() as u16;
                if w == 0 { 1 } else { w.div_ceil(area.width) }
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

fn render_entry<'a>(entry: &TimelineEntry, theme: &ChatTheme, _width: u16) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let marker_style = marker_style_for(entry, theme);

    match &entry.kind {
        EntryKind::Message => {
            if !entry.body.is_empty() {
                if entry.actor == Actor::User {
                    // User messages: `›` prefix, plain bold text (no markdown)
                    let mut body_lines = entry.body.lines();
                    if let Some(first) = body_lines.next() {
                        lines.push(Line::from(vec![
                            Span::raw(MARGIN),
                            Span::styled("› ", marker_style),
                            Span::styled(first.to_owned(), theme.bold_text),
                        ]));
                    }
                    for cont in body_lines {
                        lines.push(Line::from(vec![
                            Span::raw(INDENT),
                            Span::styled(cont.to_owned(), theme.bold_text),
                        ]));
                    }
                } else {
                    // Assistant messages: render markdown with Solarized theme
                    let opts = tui_markdown::Options::new(ThemeStyleSheet::from_chat_theme(theme));
                    let md_text = tui_markdown::from_str_with_options(&entry.body, &opts);
                    for (i, md_line) in md_text.lines.into_iter().enumerate() {
                        let owned_spans: Vec<Span<'a>> = md_line
                            .spans
                            .into_iter()
                            .map(|s| {
                                // Strip heading `# ` prefixes — tui-markdown keeps them raw
                                let content = s.content.into_owned();
                                let trimmed = content.trim_start_matches('#');
                                if trimmed.len() < content.len() {
                                    Span::styled(trimmed.trim_start().to_owned(), s.style)
                                } else {
                                    Span::styled(content, s.style)
                                }
                            })
                            .filter(|s| !s.content.is_empty())
                            .collect();

                        if i == 0 {
                            let mut first =
                                vec![Span::raw(MARGIN), Span::styled("● ", marker_style)];
                            first.extend(owned_spans);
                            lines.push(Line::from(first));
                        } else {
                            let mut cont = vec![Span::raw(INDENT)];
                            cont.extend(owned_spans);
                            lines.push(Line::from(cont));
                        }
                    }
                    // Fallback if markdown produced nothing
                    if lines.is_empty() {
                        lines.push(Line::from(vec![
                            Span::raw(MARGIN),
                            Span::styled("● ", marker_style),
                        ]));
                    }
                }
            } else {
                lines.push(Line::from(vec![
                    Span::raw(MARGIN),
                    Span::styled("● ", marker_style),
                ]));
            }
        }
        EntryKind::ToolUse {
            tool,
            target,
            additions,
            deletions,
        } => {
            let mut spans = vec![
                Span::raw(MARGIN),
                Span::styled("● ", marker_style),
                Span::styled(tool.clone(), theme.tool_label),
                Span::raw(" "),
                Span::styled(target.clone(), theme.tool_target),
            ];
            if *additions > 0 {
                spans.push(Span::styled(format!(" +{additions}"), theme.diff_add));
            }
            if *deletions > 0 {
                spans.push(Span::styled(format!(" -{deletions}"), theme.diff_del));
            }
            lines.push(Line::from(spans));

            if !entry.body.is_empty() {
                for desc_line in entry.body.lines() {
                    lines.push(Line::from(vec![
                        Span::raw(INDENT),
                        Span::styled(desc_line.to_owned(), theme.dim_text),
                    ]));
                }
            }
        }
        EntryKind::Bash { command } => {
            lines.push(Line::from(vec![
                Span::raw(MARGIN),
                Span::styled("● ", marker_style),
                Span::styled("Bash", theme.tool_label),
                Span::styled(" (shell)", theme.dim_text),
            ]));

            lines.push(Line::from(vec![
                Span::raw(INDENT),
                Span::styled(command.clone(), theme.code_block),
            ]));
            for output_line in entry.body.lines() {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::raw("  "),
                    Span::styled(output_line.to_owned(), theme.dim_text),
                ]));
            }
        }
        EntryKind::Warning => {
            let mut body_lines = entry.body.lines();
            let first = body_lines.next().unwrap_or("");
            lines.push(Line::from(vec![
                Span::raw(MARGIN),
                Span::styled("! ", marker_style),
                Span::styled(first.to_owned(), theme.body_text),
            ]));
            for continuation in body_lines {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(continuation.to_owned(), theme.body_text),
                ]));
            }
        }
        EntryKind::Hint => {
            let mut body_lines = entry.body.lines();
            let first = body_lines.next().unwrap_or("");
            lines.push(Line::from(vec![
                Span::raw(MARGIN),
                Span::styled("● ", marker_style),
                Span::styled("💡 ", marker_style),
                Span::styled(first.to_owned(), theme.body_text),
            ]));
            for continuation in body_lines {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(continuation.to_owned(), theme.body_text),
                ]));
            }
        }
        EntryKind::Thinking => {
            // Collapse newlines into spaces so thinking renders as a single
            // wrapped paragraph instead of breaking at every SSE chunk boundary.
            let collapsed = entry.body.replace('\n', " ");
            lines.push(Line::from(vec![
                Span::raw(MARGIN),
                Span::styled("● ", marker_style),
                Span::styled(collapsed, theme.dim_text),
            ]));
        }
        EntryKind::CodeBlock { language } => {
            if !language.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(language.clone(), theme.dim_text),
                ]));
            }
            for code_line in entry.body.lines() {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(code_line.to_owned(), theme.code_block),
                ]));
            }
        }
    }

    lines
}

fn marker_style_for(entry: &TimelineEntry, theme: &ChatTheme) -> Style {
    match (&entry.actor, &entry.kind) {
        (Actor::User, _) => theme.marker_response,
        (_, EntryKind::Warning) => theme.marker_warning,
        (_, EntryKind::Hint) => theme.marker_hint,
        (_, EntryKind::Thinking) => theme.marker_thinking,
        (_, EntryKind::ToolUse { .. } | EntryKind::Bash { .. }) => theme.marker_tool,
        (_, EntryKind::CodeBlock { .. }) => theme.marker_tool,
        _ => theme.marker_response,
    }
}
