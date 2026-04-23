#![allow(dead_code)]

use crate::chat::model::{EntryKind, TimelineEntry, ToolCallStatus};
use super::super::common::extract_tool_subtitle;

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

pub(crate) fn tool_descriptor(entry: &TimelineEntry) -> Option<EntryDescriptor> {
    let EntryKind::ToolCall {
        call_id,
        tool_name,
        status,
        ..
    } = &entry.kind
    else {
        return None;
    };

    let status_visual = match status {
        ToolCallStatus::Running => StatusVisual::RunningDot,
        ToolCallStatus::Success => StatusVisual::EndedDot,
        ToolCallStatus::Error => StatusVisual::ErrorX,
    };

    let subtitle = extract_tool_subtitle(tool_name, &entry.details);

    Some(EntryDescriptor {
        status_visual,
        title: tool_name.clone(),
        subtitle: (!subtitle.is_empty()).then_some(subtitle),
        preview_lines: Vec::new(),
        result_mode: ResultMode::Plain,
        call_identity: Some(call_id.clone()),
    })
}
