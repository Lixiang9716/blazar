use super::descriptor::{EntryDescriptor, StatusVisual};
use super::*;
use crate::chat::view::timeline::render_entry::common::tool_badge;

pub(crate) fn status_marker(
    status_visual: StatusVisual,
    theme: &ChatTheme,
) -> (&'static str, Style) {
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
    width: u16,
    _marker_style: Style,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    let EntryKind::ToolCall { kind, .. } = &entry.kind else {
        return lines;
    };

    let (status_marker, status_style) = status_marker(descriptor.status_visual, theme);

    let mut header = vec![
        Span::raw(MARGIN),
        Span::styled(format!("{status_marker} "), status_style),
        Span::styled(descriptor.title.clone(), theme.tool_label),
    ];
    if let Some(badge) = tool_badge(*kind) {
        header.push(Span::raw(" "));
        header.push(Span::styled(badge, theme.dim_text));
    }
    lines.push(Line::from(header));

    if let Some(subtitle) = &descriptor.subtitle {
        lines.push(Line::from(vec![
            Span::raw(INDENT),
            Span::styled(subtitle.clone(), theme.tool_target),
        ]));
    }

    let mut preview_text = descriptor.preview_lines.join("\n");
    if preview_text.trim_start().starts_with("```") && preview_text.matches("```").count() % 2 == 1
    {
        preview_text.push_str("\n```");
    }

    let preview = super::super::markdown_body::render_markdown_block_preserve_lines(
        &preview_text,
        theme,
        width,
        vec![Span::raw(INDENT)],
        vec![Span::raw(INDENT)],
    );
    lines.extend(preview);

    lines
}
