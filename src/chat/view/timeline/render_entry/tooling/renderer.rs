use super::descriptor::{EntryDescriptor, ResultMode, StatusVisual};
use super::*;
use crate::chat::view::timeline::render_entry::common::tool_badge;
use unicode_width::UnicodeWidthStr;

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
    _marker_style: Style,
    width: u16,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    let EntryKind::ToolCall { kind, .. } = &entry.kind else {
        return lines;
    };

    let (status_marker, status_style) = status_marker(descriptor.status_visual, theme);

    let badge = tool_badge(*kind);

    let mut header = vec![
        Span::raw(MARGIN),
        Span::styled(format!("{status_marker} "), status_style),
        Span::styled(descriptor.title.clone(), theme.tool_label),
    ];
    if let Some(badge) = badge {
        header.push(Span::raw(" "));
        header.push(Span::styled(badge, theme.dim_text));
    }

    let left_width = MARGIN.width()
        + status_marker.width()
        + 1
        + descriptor.title.width()
        + badge.map_or(0, UnicodeWidthStr::width)
        + badge.map_or(0, |_| 1);

    if let Some(inline_parameter) = descriptor.inline_parameter.as_deref() {
        let slot_width = (width as usize).saturating_sub(left_width);
        let fitted_parameter =
            super::super::common::truncate_display_width(inline_parameter, slot_width);
        if !fitted_parameter.is_empty() {
            let gap = slot_width.saturating_sub(fitted_parameter.width());
            if gap > 0 {
                header.push(Span::raw(" ".repeat(gap)));
            }
            header.push(Span::styled(fitted_parameter, theme.tool_target));
        }
    }

    lines.push(Line::from(header));

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
