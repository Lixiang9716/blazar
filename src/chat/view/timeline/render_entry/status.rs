use super::super::text_wrap::push_wrapped_lines;
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
    _entry: &TimelineEntry,
    _theme: &ChatTheme,
    _width: u16,
) -> Vec<Line<'a>> {
    Vec::new()
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
