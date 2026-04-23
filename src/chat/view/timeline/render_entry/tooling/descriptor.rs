#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusVisual {
    RunningDot,
    EndedDot,
    ErrorX,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResultMode {
    Markdown,
    Code,
    Diff,
    Plain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntryDescriptor {
    pub status_visual: StatusVisual,
    pub title: String,
    pub subtitle: Option<String>,
    pub preview_lines: Vec<String>,
    pub result_mode: ResultMode,
    pub call_identity: Option<String>,
}

impl EntryDescriptor {
    pub(super) fn call_identity_suffix(&self) -> Option<&str> {
        self.call_identity.as_deref()
    }
}

use crate::chat::model::{EntryKind, TimelineEntry, ToolCallStatus};
use crate::chat::view::timeline::render_entry::common::extract_tool_subtitle;

const MAX_PREVIEW_LINES: usize = 2;

pub(super) fn tool_descriptor(entry: &TimelineEntry) -> EntryDescriptor {
    let EntryKind::ToolCall {
        call_id,
        tool_name,
        status,
        ..
    } = &entry.kind
    else {
        unreachable!("tool_descriptor only handles tool call entries");
    };

    let status_visual = match status {
        ToolCallStatus::Running => StatusVisual::RunningDot,
        ToolCallStatus::Success => StatusVisual::EndedDot,
        ToolCallStatus::Error => StatusVisual::ErrorX,
    };

    let subtitle = extract_tool_subtitle(tool_name, &entry.details);

    EntryDescriptor {
        status_visual,
        title: tool_name.clone(),
        subtitle: (!subtitle.is_empty()).then_some(subtitle),
        preview_lines: build_preview_lines(&entry.body),
        result_mode: infer_result_mode(tool_name, &entry.body),
        call_identity: Some(call_id.clone()),
    }
}

fn build_preview_lines(text: &str) -> Vec<String> {
    text.lines()
        .take(MAX_PREVIEW_LINES)
        .map(ToOwned::to_owned)
        .collect()
}

fn infer_result_mode(tool_name: &str, text: &str) -> ResultMode {
    if tool_name == "edit_file" || text.contains("diff --git") || text.contains("@@") {
        return ResultMode::Diff;
    }

    if text.contains("```") {
        return ResultMode::Code;
    }

    if text.contains("# ") || text.contains("- ") {
        return ResultMode::Markdown;
    }

    ResultMode::Plain
}
