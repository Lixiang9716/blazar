use super::markdown::{MdSegment, normalize_markdown_paragraphs, split_code_fences};
use super::text_wrap::{push_wrapped_lines, wrap_text};
use super::*;

/// Render a fenced code block with language label, background, and borders.
pub(super) fn render_fenced_code<'a>(
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

pub(super) fn render_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
) -> Vec<Line<'a>> {
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

            // Show key argument (file path / command) as a subtitle
            let subtitle = extract_tool_subtitle(tool_name, &entry.details);
            if !subtitle.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(subtitle, theme.tool_target),
                ]));
            }

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

            // Output lines — cap to last MAX_BASH_OUTPUT_LINES
            const MAX_BASH_OUTPUT_LINES: usize = 8;
            let output_lines: Vec<&str> = entry.body.lines().collect();
            let (shown, skipped) = if output_lines.len() > MAX_BASH_OUTPUT_LINES {
                (
                    &output_lines[output_lines.len() - MAX_BASH_OUTPUT_LINES..],
                    output_lines.len() - MAX_BASH_OUTPUT_LINES,
                )
            } else {
                (output_lines.as_slice(), 0)
            };
            if skipped > 0 {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(
                        format!("  … {skipped} lines hidden (Ctrl+O to expand)"),
                        theme.dim_text,
                    ),
                ]));
            }
            for output_line in shown {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::raw("  "),
                    Span::styled((*output_line).to_owned(), theme.dim_text),
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
            // Bordered thinking block — shows first few lines collapsed.
            // Full content available via Ctrl+O detail toggle.
            let text_width = width.saturating_sub(INDENT_WIDTH);
            let w = text_width as usize;
            let border_style = theme.marker_thinking;
            let content_style = Style::default()
                .fg(theme.dim_text.fg.unwrap_or(Color::Reset))
                .bg(theme.code_bg);

            // Top border with label
            let label = " 🧠 Thinking ";
            let label_w = UnicodeWidthStr::width(label);
            let bar_len = w.saturating_sub(label_w);
            lines.push(Line::from(vec![
                Span::raw(INDENT),
                Span::styled(label, border_style),
                Span::styled("─".repeat(bar_len), border_style),
            ]));

            // Content — show first MAX_THINKING_LINES
            const MAX_THINKING_LINES: usize = 4;
            let body = entry.body.replace('\n', " ");
            let all_lines = wrap_text(&body, text_width);
            let total = all_lines.len();
            let shown = total.min(MAX_THINKING_LINES);
            for line_text in &all_lines[..shown] {
                let display_w = UnicodeWidthStr::width(line_text.as_str());
                let padding = w.saturating_sub(display_w);
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(format!("{line_text}{}", " ".repeat(padding)), content_style),
                ]));
            }
            if total > MAX_THINKING_LINES {
                let note = format!("… +{} lines (Ctrl+O)", total - shown);
                let note_w = UnicodeWidthStr::width(note.as_str());
                let note_pad = w.saturating_sub(note_w);
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(format!("{note}{}", " ".repeat(note_pad)), content_style),
                ]));
            }

            // Bottom border
            lines.push(Line::from(vec![
                Span::raw(INDENT),
                Span::styled("─".repeat(w), border_style),
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

/// Extract a short subtitle from tool-call arguments (stored in `details`).
/// Shows the most useful field — file path for read/write, command for bash.
fn extract_tool_subtitle(tool_name: &str, details: &str) -> String {
    let val: serde_json::Value = match serde_json::from_str(details) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };

    let key = match tool_name {
        "read_file" | "write_file" | "create_file" | "list_dir" => "path",
        "edit_file" => "path",
        "bash" | "shell" => "command",
        "grep" | "ripgrep" => "pattern",
        "search" | "find_files" => "query",
        _ => {
            // Fallback: try common keys in order
            for k in &["path", "file", "command", "query", "url"] {
                if let Some(s) = val.get(*k).and_then(|v| v.as_str()) {
                    return truncate_subtitle(s);
                }
            }
            return String::new();
        }
    };

    val.get(key)
        .and_then(|v| v.as_str())
        .map(truncate_subtitle)
        .unwrap_or_default()
}

fn truncate_subtitle(s: &str) -> String {
    if s.len() > 80 {
        format!("{}…", &s[..77])
    } else {
        s.to_owned()
    }
}
