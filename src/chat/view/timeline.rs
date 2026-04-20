use crate::chat::app::ChatApp;
use crate::chat::model::{Actor, EntryKind, TimelineEntry};
use crate::chat::theme::ChatTheme;
use core::cmp;
use ratatui_core::{
    layout::Rect,
    style::Style,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::paragraph::{Paragraph, Wrap};
use unicode_width::UnicodeWidthChar;

/// Left margin for all timeline content (matches Claude Code's indentation).
const MARGIN: &str = "  ";
/// Continuation indent (margin + marker width).
const INDENT: &str = "    ";
const INDENT_WIDTH: u16 = 4;

pub(super) fn render_timeline(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let mut lines: Vec<Line> = Vec::new();
    let show_details = app.show_details();

    let content_width = area.width;

    for entry in app.timeline() {
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

fn render_entry<'a>(entry: &TimelineEntry, theme: &ChatTheme, width: u16) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let marker_style = marker_style_for(entry, theme);

    match &entry.kind {
        EntryKind::Message => {
            if !entry.body.is_empty() {
                if entry.actor == Actor::User {
                    // User messages: `›` prefix, plain bold text (no markdown)
                    for (i, body_line) in entry.body.lines().enumerate() {
                        let prefix = if i == 0 {
                            vec![Span::raw(MARGIN), Span::styled("› ", marker_style)]
                        } else {
                            vec![Span::raw(INDENT)]
                        };
                        push_wrapped_lines(&mut lines, body_line, theme.bold_text, prefix, width);
                    }
                } else {
                    // Assistant messages: render markdown via ratskin (termimad backend)
                    // Normalize soft breaks first — termimad treats every \n as hard break.
                    let body = entry.body.trim();
                    if body.is_empty() {
                        lines.push(Line::from(vec![
                            Span::raw(MARGIN),
                            Span::styled("● ", marker_style),
                        ]));
                    } else {
                        let normalized = normalize_markdown_paragraphs(body);
                        let rat_skin = ratskin::RatSkin {
                            skin: theme.mad_skin.clone(),
                        };
                        let text_width = width.saturating_sub(INDENT_WIDTH);
                        let md_lines =
                            rat_skin.parse(ratskin::RatSkin::parse_text(&normalized), text_width);

                        for (i, md_line) in md_lines.into_iter().enumerate() {
                            let prefix = if i == 0 {
                                vec![Span::raw(MARGIN), Span::styled("● ", marker_style)]
                            } else {
                                vec![Span::raw(INDENT)]
                            };
                            let mut result_spans = prefix;
                            result_spans.extend(md_line.spans);
                            lines.push(Line::from(result_spans));
                        }
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
            for (i, body_line) in entry.body.lines().enumerate() {
                let prefix = if i == 0 {
                    vec![Span::raw(MARGIN), Span::styled("! ", marker_style)]
                } else {
                    vec![Span::raw(INDENT)]
                };
                push_wrapped_lines(&mut lines, body_line, theme.body_text, prefix, width);
            }
        }
        EntryKind::Hint => {
            for (i, body_line) in entry.body.lines().enumerate() {
                let prefix = if i == 0 {
                    vec![
                        Span::raw(MARGIN),
                        Span::styled("● ", marker_style),
                        Span::styled("💡 ", marker_style),
                    ]
                } else {
                    vec![Span::raw(INDENT)]
                };
                push_wrapped_lines(&mut lines, body_line, theme.body_text, prefix, width);
            }
        }
        EntryKind::Thinking => {
            // Show a compact single-line summary with truncation.
            // Full thinking text is available via Ctrl+O detail toggle.
            let collapsed = entry.body.replace('\n', " ");
            let max_chars = 60;
            let summary = if collapsed.chars().count() > max_chars {
                let truncated: String = collapsed.chars().take(max_chars).collect();
                format!("{truncated}…")
            } else {
                collapsed
            };
            lines.push(Line::from(vec![
                Span::raw(MARGIN),
                Span::styled("● ", marker_style),
                Span::styled("Thinking ", theme.dim_text),
                Span::styled(summary, theme.dim_text),
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

/// Break `text` into chunks that each fit within `max_cols` display columns.
fn wrap_text(text: &str, max_cols: u16) -> Vec<String> {
    if max_cols == 0 {
        return vec![text.to_owned()];
    }
    let max = max_cols as usize;
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut col = 0usize;

    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if col + w > max && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            col = 0;
        }
        current.push(ch);
        col += w;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() {
        chunks.push(String::new());
    }
    chunks
}

/// Emit wrapped Lines: first line gets `prefix` spans, continuations get INDENT.
fn push_wrapped_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    text: &str,
    style: Style,
    prefix: Vec<Span<'a>>,
    max_width: u16,
) {
    // Prefix display width: MARGIN(2) + marker(2) = 4 typically
    let text_width = max_width.saturating_sub(INDENT_WIDTH);
    let chunks = wrap_text(text, text_width);

    for (i, chunk) in chunks.into_iter().enumerate() {
        if i == 0 {
            let mut spans = prefix.clone();
            spans.push(Span::styled(chunk, style));
            lines.push(Line::from(spans));
        } else {
            lines.push(Line::from(vec![
                Span::raw(INDENT),
                Span::styled(chunk, style),
            ]));
        }
    }
}

/// Join soft line-breaks within paragraphs so termimad doesn't treat them as
/// hard breaks.  Preserves structural markdown elements (headings, lists,
/// code fences, tables, blank lines) and code-block interiors.
fn normalize_markdown_paragraphs(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let lines: Vec<&str> = text.split('\n').collect();
    let mut in_code_block = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
        }

        result.push_str(line);

        if i + 1 < lines.len() {
            if in_code_block {
                result.push('\n');
                continue;
            }
            let next_trimmed = lines[i + 1].trim();
            if is_structural_line(trimmed) || is_structural_line(next_trimmed) {
                result.push('\n');
            } else {
                result.push(' ');
            }
        }
    }
    result
}

fn is_structural_line(s: &str) -> bool {
    s.is_empty()
        || s.starts_with('#')
        || s.starts_with("- ")
        || s.starts_with("* ")
        || s.starts_with("+ ")
        || s.starts_with("```")
        || s.starts_with("> ")
        || s.starts_with('|')
        || s.starts_with("---")
        || s.starts_with("===")
        || s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() && s.contains(". "))
}
