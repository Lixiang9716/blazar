use super::descriptor::{EntryDescriptor, ResultMode, StatusVisual};
use super::*;
use crate::chat::view::timeline::render_entry::common::tool_badge;

pub(crate) fn status_marker(status_visual: StatusVisual, theme: &ChatTheme) -> (&'static str, Style) {
    match status_visual {
        StatusVisual::RunningDot => ("●", theme.spinner),
        StatusVisual::EndedDot => ("●", theme.diff_add),
        StatusVisual::ErrorX => ("x", theme.marker_warning),
    }
}

pub(super) fn render_tool_descriptor<'a>(
    descriptor: &EntryDescriptor,
    entry: &TimelineEntry,
    theme: &ChatTheme,
    marker_style: Style,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    let EntryKind::ToolCall { kind, .. } = &entry.kind else {
        return lines;
    };

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
    if let Some(call_identity) = descriptor.call_identity_suffix() {
        header.push(Span::raw(" "));
        header.push(Span::styled(format!("[{call_identity}]"), theme.dim_text));
    }
    header.extend([Span::raw(" "), Span::styled(status_marker, status_style)]);
    lines.push(Line::from(header));

    if let Some(subtitle) = &descriptor.subtitle {
        lines.push(Line::from(vec![
            Span::raw(INDENT),
            Span::styled(subtitle.clone(), theme.tool_target),
        ]));
    }

    let preview_style = match descriptor.result_mode {
        ResultMode::Diff => theme.diff_add,
        ResultMode::Markdown => theme.dim_text,
        ResultMode::Code => theme.code_block,
        ResultMode::Plain => theme.dim_text,
    };

    for preview_line in &descriptor.preview_lines {
        lines.push(Line::from(vec![
            Span::raw(INDENT),
            Span::styled(preview_line.clone(), preview_style),
        ]));
    }

    lines
}
