use super::*;

mod banner;
mod common;
mod fenced_code;
mod markdown_body;
mod message;
mod status;
mod tooling;

#[cfg(test)]
#[path = "../../../../tests/unit/chat/view/timeline/render_entry/tests.rs"]
mod tests;

use common::marker_style_for;
pub(super) use markdown_body::render_markdown_details_block;

pub(super) trait TimelineEntryRenderer {
    fn render(&self, entry: &TimelineEntry, theme: &ChatTheme, width: u16) -> Vec<Line<'static>>;
}

trait EntryKindRenderer {
    fn supports(&self, kind: &EntryKind) -> bool;
    fn render(
        &self,
        entry: &TimelineEntry,
        theme: &ChatTheme,
        width: u16,
        marker_style: Style,
    ) -> Vec<Line<'static>>;
}

pub(super) struct EntryRenderRegistry {
    renderers: Vec<Box<dyn EntryKindRenderer>>,
}

impl Default for EntryRenderRegistry {
    fn default() -> Self {
        Self {
            renderers: vec![
                Box::new(MessageRenderer),
                Box::new(ToolUseRenderer),
                Box::new(ToolCallRenderer),
                Box::new(BashRenderer),
                Box::new(WarningRenderer),
                Box::new(HintRenderer),
                Box::new(ThinkingRenderer),
                Box::new(CodeBlockRenderer),
                Box::new(BannerRenderer),
            ],
        }
    }
}

impl TimelineEntryRenderer for EntryRenderRegistry {
    fn render(&self, entry: &TimelineEntry, theme: &ChatTheme, width: u16) -> Vec<Line<'static>> {
        let marker_style = marker_style_for(entry, theme);
        self.renderers
            .iter()
            .find(|renderer| renderer.supports(&entry.kind))
            .map(|renderer| renderer.render(entry, theme, width, marker_style))
            .unwrap_or_default()
    }
}

struct MessageRenderer;
struct ToolUseRenderer;
struct ToolCallRenderer;
struct BashRenderer;
struct WarningRenderer;
struct HintRenderer;
struct ThinkingRenderer;
struct CodeBlockRenderer;
struct BannerRenderer;

impl EntryKindRenderer for MessageRenderer {
    fn supports(&self, kind: &EntryKind) -> bool {
        matches!(kind, EntryKind::Message)
    }

    fn render(
        &self,
        entry: &TimelineEntry,
        theme: &ChatTheme,
        width: u16,
        marker_style: Style,
    ) -> Vec<Line<'static>> {
        message::render_message_entry(entry, theme, width, marker_style)
    }
}

impl EntryKindRenderer for ToolUseRenderer {
    fn supports(&self, kind: &EntryKind) -> bool {
        matches!(kind, EntryKind::ToolUse { .. })
    }

    fn render(
        &self,
        entry: &TimelineEntry,
        theme: &ChatTheme,
        width: u16,
        marker_style: Style,
    ) -> Vec<Line<'static>> {
        tooling::render_tool_use_entry(entry, theme, width, marker_style)
    }
}

impl EntryKindRenderer for ToolCallRenderer {
    fn supports(&self, kind: &EntryKind) -> bool {
        matches!(kind, EntryKind::ToolCall { .. })
    }

    fn render(
        &self,
        entry: &TimelineEntry,
        theme: &ChatTheme,
        width: u16,
        marker_style: Style,
    ) -> Vec<Line<'static>> {
        tooling::render_tool_call_entry(entry, theme, width, marker_style)
    }
}

impl EntryKindRenderer for BashRenderer {
    fn supports(&self, kind: &EntryKind) -> bool {
        matches!(kind, EntryKind::Bash { .. })
    }

    fn render(
        &self,
        entry: &TimelineEntry,
        theme: &ChatTheme,
        width: u16,
        marker_style: Style,
    ) -> Vec<Line<'static>> {
        tooling::render_bash_entry(entry, theme, width, marker_style)
    }
}

impl EntryKindRenderer for WarningRenderer {
    fn supports(&self, kind: &EntryKind) -> bool {
        matches!(kind, EntryKind::Warning)
    }

    fn render(
        &self,
        entry: &TimelineEntry,
        theme: &ChatTheme,
        width: u16,
        marker_style: Style,
    ) -> Vec<Line<'static>> {
        status::render_warning_entry(entry, theme, width, marker_style)
    }
}

impl EntryKindRenderer for HintRenderer {
    fn supports(&self, kind: &EntryKind) -> bool {
        matches!(kind, EntryKind::Hint)
    }

    fn render(
        &self,
        entry: &TimelineEntry,
        theme: &ChatTheme,
        width: u16,
        marker_style: Style,
    ) -> Vec<Line<'static>> {
        status::render_hint_entry(entry, theme, width, marker_style)
    }
}

impl EntryKindRenderer for ThinkingRenderer {
    fn supports(&self, kind: &EntryKind) -> bool {
        matches!(kind, EntryKind::Thinking)
    }

    fn render(
        &self,
        entry: &TimelineEntry,
        theme: &ChatTheme,
        width: u16,
        _marker_style: Style,
    ) -> Vec<Line<'static>> {
        status::render_thinking_entry(entry, theme, width)
    }
}

impl EntryKindRenderer for CodeBlockRenderer {
    fn supports(&self, kind: &EntryKind) -> bool {
        matches!(kind, EntryKind::CodeBlock { .. })
    }

    fn render(
        &self,
        entry: &TimelineEntry,
        theme: &ChatTheme,
        width: u16,
        _marker_style: Style,
    ) -> Vec<Line<'static>> {
        status::render_code_block_entry(entry, theme, width)
    }
}

impl EntryKindRenderer for BannerRenderer {
    fn supports(&self, kind: &EntryKind) -> bool {
        matches!(kind, EntryKind::Banner)
    }

    fn render(
        &self,
        entry: &TimelineEntry,
        theme: &ChatTheme,
        width: u16,
        _marker_style: Style,
    ) -> Vec<Line<'static>> {
        let workspace = &entry.body;
        let branch = &entry.details;
        banner::render_banner_entry(theme, width, workspace, branch)
    }
}

#[cfg(test)]
pub(super) fn render_fenced_code<'a>(
    lang: &str,
    code: &str,
    theme: &ChatTheme,
    text_width: u16,
) -> Vec<Line<'a>> {
    fenced_code::render_fenced_code(lang, code, theme, text_width)
}

#[cfg(test)]
pub(super) fn render_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
) -> Vec<Line<'a>> {
    EntryRenderRegistry::default().render(entry, theme, width)
}
