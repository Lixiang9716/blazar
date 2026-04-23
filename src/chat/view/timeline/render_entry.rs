use super::*;

mod common;
mod message;
mod status;
mod tooling;

#[cfg(test)]
#[path = "../../../../tests/unit/chat/view/timeline/render_entry/tests.rs"]
mod tests;

use common::marker_style_for;

#[cfg(test)]
pub(super) fn render_fenced_code<'a>(
    lang: &str,
    code: &str,
    theme: &ChatTheme,
    text_width: u16,
) -> Vec<Line<'a>> {
    message::render_fenced_code(lang, code, theme, text_width)
}

pub(super) fn render_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
) -> Vec<Line<'a>> {
    let marker_style = marker_style_for(entry, theme);

    match &entry.kind {
        EntryKind::Message => message::render_message_entry(entry, theme, width, marker_style),
        EntryKind::ToolUse { .. } => tooling::render_tool_use_entry(entry, theme, marker_style),
        EntryKind::ToolCall { .. } => tooling::render_tool_call_entry(entry, theme, marker_style),
        EntryKind::Bash { .. } => tooling::render_bash_entry(entry, theme, width, marker_style),
        EntryKind::Warning => status::render_warning_entry(entry, theme, width, marker_style),
        EntryKind::Hint => status::render_hint_entry(entry, theme, width, marker_style),
        EntryKind::Thinking => status::render_thinking_entry(entry, theme, width),
        EntryKind::CodeBlock { .. } => status::render_code_block_entry(entry, theme, width),
    }
}
