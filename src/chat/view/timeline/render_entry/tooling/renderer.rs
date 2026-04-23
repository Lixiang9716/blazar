use super::common::tool_badge;
use super::*;

pub(crate) fn status_marker(
    status_visual: descriptor::StatusVisual,
    theme: &ChatTheme,
) -> (&'static str, Style) {
    match status_visual {
        descriptor::StatusVisual::RunningDot => ("●", theme.spinner),
        descriptor::StatusVisual::EndedDot => ("●", theme.diff_add),
        descriptor::StatusVisual::ErrorX => ("x", theme.marker_warning),
    }
}

pub(super) fn render_tool_call_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    marker_style: Style,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    let Some(descriptor) = tool_descriptor(entry) else {
        return lines;
    };

    if let EntryKind::ToolCall { kind, .. } = &entry.kind {
        let (status_marker, status_style) = status_marker(descriptor.status_visual, theme);

        let mut header = vec![
            Span::raw(MARGIN),
            Span::styled("● ", marker_style),
            Span::styled(descriptor.title.clone(), theme.tool_label),
        ];
        if let Some(badge) = tool_badge(*kind) {
            header.push(Span::raw(" "));
            header.push(Span::styled(badge, theme.dim_text));
        }
        header.extend([Span::raw(" "), Span::styled(status_marker, status_style)]);
        lines.push(Line::from(header));

        if let Some(subtitle) = descriptor.subtitle {
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
