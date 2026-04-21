use super::common::extract_tool_subtitle;
use super::*;

pub(super) fn render_tool_use_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    marker_style: Style,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    if let EntryKind::ToolUse {
        tool,
        target,
        additions,
        deletions,
    } = &entry.kind
    {
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

    lines
}

pub(super) fn render_tool_call_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    marker_style: Style,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    if let EntryKind::ToolCall {
        tool_name, status, ..
    } = &entry.kind
    {
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

    lines
}

pub(super) fn render_bash_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
    marker_style: Style,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    let EntryKind::Bash { command } = &entry.kind else {
        return lines;
    };

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

    lines
}
