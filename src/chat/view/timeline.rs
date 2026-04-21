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

/// Markdown segment: either plain text or a fenced code block.
enum MdSegment {
    Text(String),
    Code { lang: String, body: String },
}

/// Split markdown into text and fenced-code segments.
/// Unfenced content (including indented code blocks) stays as Text.
fn split_code_fences(src: &str) -> Vec<MdSegment> {
    let mut segments = Vec::new();
    let mut text_buf = String::new();
    let mut lines = src.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            // Flush accumulated text
            if !text_buf.is_empty() {
                segments.push(MdSegment::Text(std::mem::take(&mut text_buf)));
            }
            let lang = trimmed.trim_start_matches('`').trim().to_owned();
            let mut code_buf = String::new();
            let mut closed = false;

            for code_line in lines.by_ref() {
                if code_line.trim().starts_with("```") {
                    closed = true;
                    break;
                }
                if !code_buf.is_empty() {
                    code_buf.push('\n');
                }
                code_buf.push_str(code_line);
            }

            if closed {
                segments.push(MdSegment::Code {
                    lang,
                    body: code_buf,
                });
            } else {
                // Unclosed fence — treat original lines as text
                text_buf.push_str("```");
                text_buf.push_str(&lang);
                text_buf.push('\n');
                text_buf.push_str(&code_buf);
            }
        } else {
            if !text_buf.is_empty() {
                text_buf.push('\n');
            }
            text_buf.push_str(line);
        }
    }

    if !text_buf.is_empty() {
        segments.push(MdSegment::Text(text_buf));
    }

    // Guarantee at least one segment
    if segments.is_empty() {
        segments.push(MdSegment::Text(String::new()));
    }

    segments
}

/// Render a fenced code block with language label, background, and borders.
fn render_fenced_code<'a>(
    lang: &str,
    code: &str,
    theme: &ChatTheme,
    text_width: u16,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let w = text_width as usize;
    let code_style = Style::default()
        .fg(theme.code_block.fg.unwrap_or(Color::Reset))
        .bg(theme.code_bg);
    let border_style = theme.dim_text;

    // Top border with optional language label
    let top = if !lang.is_empty() {
        let label = format!(" {lang} ");
        let label_w = UnicodeWidthStr::width(label.as_str());
        let bar_len = w.saturating_sub(label_w);
        format!("{label}{}", "─".repeat(bar_len))
    } else {
        "─".repeat(w)
    };
    lines.push(Line::from(Span::styled(top, border_style)));

    // Code lines with full-width background
    if code.is_empty() {
        lines.push(Line::from(Span::styled(" ".repeat(w), code_style)));
    } else {
        for code_line in code.lines() {
            let expanded = code_line.replace('\t', "    ");
            let display_w = UnicodeWidthStr::width(expanded.as_str());
            let padding = w.saturating_sub(display_w);
            lines.push(Line::from(Span::styled(
                format!("{expanded}{}", " ".repeat(padding)),
                code_style,
            )));
        }
    }

    // Bottom border
    lines.push(Line::from(Span::styled("─".repeat(w), border_style)));
    lines
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
                        // Assistant messages: split code fences for custom rendering,
                        // render remaining text via ratskin (termimad backend).
                        let segments = split_code_fences(body);
                        let rat_skin = ratskin::RatSkin {
                            skin: theme.mad_skin.clone(),
                        };
                        let text_width = width.saturating_sub(INDENT_WIDTH);
                        let mut is_first_line = true;

                        for segment in &segments {
                            match segment {
                                MdSegment::Text(text) => {
                                    let trimmed_seg = text.trim();
                                    if trimmed_seg.is_empty() {
                                        continue;
                                    }
                                    let normalized = normalize_markdown_paragraphs(trimmed_seg);
                                    let md_lines = rat_skin.parse(
                                        ratskin::RatSkin::parse_text(&normalized),
                                        text_width,
                                    );
                                    for md_line in md_lines {
                                        let prefix = if is_first_line {
                                            is_first_line = false;
                                            vec![
                                                Span::raw(MARGIN),
                                                Span::styled("● ", marker_style),
                                            ]
                                        } else {
                                            vec![Span::raw(INDENT)]
                                        };
                                        let mut result_spans = prefix;
                                        result_spans.extend(md_line.spans);
                                        lines.push(Line::from(result_spans));
                                    }
                                }
                                MdSegment::Code { lang, body: code } => {
                                    if is_first_line {
                                        is_first_line = false;
                                        lines.push(Line::from(vec![
                                            Span::raw(MARGIN),
                                            Span::styled("● ", marker_style),
                                        ]));
                                    }
                                    let code_lines =
                                        render_fenced_code(lang, code, theme, text_width);
                                    for code_line in code_lines {
                                        let mut result_spans = vec![Span::raw(INDENT)];
                                        result_spans.extend(code_line.spans);
                                        lines.push(Line::from(result_spans));
                                    }
                                }
                            }
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
        EntryKind::ToolCall {
            tool_name, status, ..
        } => {
            let (status_marker, status_style) = match status {
                ToolCallStatus::Running => ("…", theme.spinner),
                ToolCallStatus::Success => ("✓", theme.diff_add),
                ToolCallStatus::Error => ("✗", theme.marker_warning),
            };

            lines.push(Line::from(vec![
                Span::raw(MARGIN),
                Span::styled("● ", marker_style),
                Span::styled(tool_name.clone(), theme.tool_label),
                Span::raw(" "),
                Span::styled(status_marker, status_style),
            ]));

            for body_line in entry.body.lines() {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(body_line.to_owned(), theme.dim_text),
                ]));
            }
        }
        EntryKind::Bash { command } => {
            lines.push(Line::from(vec![
                Span::raw(MARGIN),
                Span::styled("● ", marker_style),
                Span::styled("bash", theme.tool_label),
            ]));

            // Command line with code background and $ prompt
            let text_width = width.saturating_sub(INDENT_WIDTH) as usize;
            let cmd_display = format!("$ {command}");
            let cmd_w = UnicodeWidthStr::width(cmd_display.as_str());
            let cmd_padding = text_width.saturating_sub(cmd_w);
            let cmd_style = Style::default()
                .fg(theme.code_block.fg.unwrap_or(Color::Reset))
                .bg(theme.code_bg);
            lines.push(Line::from(vec![
                Span::raw(INDENT),
                Span::styled(
                    format!("{cmd_display}{}", " ".repeat(cmd_padding)),
                    cmd_style,
                ),
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
            let max_chars = (width as usize / 3).max(30);
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
            let text_width = width.saturating_sub(INDENT_WIDTH);
            let code_lines = render_fenced_code(language, &entry.body, theme, text_width);
            for code_line in code_lines {
                let mut result_spans = vec![Span::raw(INDENT)];
                result_spans.extend(code_line.spans);
                lines.push(Line::from(result_spans));
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
        (_, EntryKind::ToolUse { .. } | EntryKind::ToolCall { .. } | EntryKind::Bash { .. }) => {
            theme.marker_tool
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_code_fences_no_code() {
        let segments = split_code_fences("Hello world\nSecond line");
        assert_eq!(segments.len(), 1);
        assert!(matches!(&segments[0], MdSegment::Text(t) if t == "Hello world\nSecond line"));
    }

    #[test]
    fn split_code_fences_single_block() {
        let input = "Before\n```python\nprint('hi')\n```\nAfter";
        let segments = split_code_fences(input);
        assert_eq!(segments.len(), 3);
        assert!(matches!(&segments[0], MdSegment::Text(t) if t == "Before"));
        assert!(
            matches!(&segments[1], MdSegment::Code { lang, body } if lang == "python" && body == "print('hi')")
        );
        assert!(matches!(&segments[2], MdSegment::Text(t) if t == "After"));
    }

    #[test]
    fn split_code_fences_multiple_blocks() {
        let input = "A\n```rust\nfn main(){}\n```\nB\n```go\nfunc main(){}\n```\nC";
        let segments = split_code_fences(input);
        assert_eq!(segments.len(), 5);
        assert!(matches!(&segments[0], MdSegment::Text(t) if t == "A"));
        assert!(matches!(&segments[1], MdSegment::Code { lang, .. } if lang == "rust"));
        assert!(matches!(&segments[2], MdSegment::Text(t) if t == "B"));
        assert!(matches!(&segments[3], MdSegment::Code { lang, .. } if lang == "go"));
        assert!(matches!(&segments[4], MdSegment::Text(t) if t == "C"));
    }

    #[test]
    fn split_code_fences_unclosed_treated_as_text() {
        let input = "Before\n```python\nprint('hi')";
        let segments = split_code_fences(input);
        // Unclosed fence falls back to text
        assert!(segments.iter().all(|s| matches!(s, MdSegment::Text(_))));
    }

    #[test]
    fn split_code_fences_empty_body() {
        let input = "```\n```";
        let segments = split_code_fences(input);
        assert_eq!(segments.len(), 1);
        assert!(
            matches!(&segments[0], MdSegment::Code { lang, body } if lang.is_empty() && body.is_empty())
        );
    }

    #[test]
    fn render_fenced_code_has_borders_and_bg() {
        let theme = crate::chat::theme::build_theme();
        let lines = render_fenced_code("python", "x = 1\ny = 2", &theme, 40);
        // top border + 2 code lines + bottom border = 4
        assert_eq!(lines.len(), 4);

        // First line contains language label
        let top_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(top_text.contains("python"));

        // Code lines have code_bg background
        for code_line in &lines[1..3] {
            let has_bg = code_line
                .spans
                .iter()
                .any(|s| s.style.bg == Some(theme.code_bg));
            assert!(has_bg, "code line should have code_bg background");
        }

        // Code lines are padded to width
        let code_text: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(UnicodeWidthStr::width(code_text.as_str()), 40);
    }

    #[test]
    fn render_fenced_code_empty_body() {
        let theme = crate::chat::theme::build_theme();
        let lines = render_fenced_code("", "", &theme, 20);
        // top border + 1 blank bg line + bottom border = 3
        assert_eq!(lines.len(), 3);
    }
}
