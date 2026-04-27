use super::super::text_wrap::push_wrapped_lines;
use super::markdown_body::render_markdown_block;
use super::*;

pub(super) fn render_message_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
    marker_style: Style,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

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
            lines.extend(render_markdown_block(
                &entry.body,
                theme,
                width,
                vec![Span::raw(MARGIN), Span::styled("● ", marker_style)],
                vec![Span::raw(INDENT)],
            ));
        }
    } else {
        return lines;
    }

    lines
}
