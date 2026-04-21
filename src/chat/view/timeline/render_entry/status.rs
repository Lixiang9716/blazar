use super::super::text_wrap::{push_wrapped_lines, wrap_text};
use super::message::render_fenced_code;
use super::*;

pub(super) fn render_warning_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
    marker_style: Style,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    for (i, body_line) in entry.body.lines().enumerate() {
        let prefix = if i == 0 {
            vec![Span::raw(MARGIN), Span::styled("! ", marker_style)]
        } else {
            vec![Span::raw(INDENT)]
        };
        push_wrapped_lines(&mut lines, body_line, theme.body_text, prefix, width);
    }

    lines
}

pub(super) fn render_hint_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
    marker_style: Style,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

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

    lines
}

pub(super) fn render_thinking_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

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

    lines
}

pub(super) fn render_code_block_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    let EntryKind::CodeBlock { language } = &entry.kind else {
        return lines;
    };

    let text_width = width.saturating_sub(INDENT_WIDTH);
    let code_lines = render_fenced_code(language, &entry.body, theme, text_width);
    for code_line in code_lines {
        let mut result_spans = vec![Span::raw(INDENT)];
        result_spans.extend(code_line.spans);
        lines.push(Line::from(result_spans));
    }

    lines
}
