use super::markdown_body::{render_markdown_block, render_markdown_block_with_text_style};
use super::*;

pub(super) fn render_warning_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
    marker_style: Style,
) -> Vec<Line<'a>> {
    render_markdown_block(
        &entry.body,
        theme,
        width,
        vec![Span::raw(MARGIN), Span::styled("! ", marker_style)],
        vec![Span::raw(INDENT)],
    )
}

pub(super) fn render_hint_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
    marker_style: Style,
) -> Vec<Line<'a>> {
    render_markdown_block(
        &entry.body,
        theme,
        width,
        vec![
            Span::raw(MARGIN),
            Span::styled("● ", marker_style),
            Span::styled("💡 ", marker_style),
        ],
        vec![Span::raw(INDENT)],
    )
}

pub(super) fn render_thinking_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
) -> Vec<Line<'a>> {
    render_markdown_block_with_text_style(
        &entry.body,
        theme,
        width,
        vec![Span::raw(MARGIN), Span::styled("… ", theme.marker_thinking)],
        vec![Span::raw(INDENT)],
        theme.marker_thinking,
    )
}

pub(super) fn render_code_block_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
) -> Vec<Line<'a>> {
    let EntryKind::CodeBlock { language } = &entry.kind else {
        return Vec::new();
    };

    let fenced = format!("```{language}\n{}\n```", entry.body);
    render_markdown_block(
        &fenced,
        theme,
        width,
        vec![Span::raw(INDENT)],
        vec![Span::raw(INDENT)],
    )
}
