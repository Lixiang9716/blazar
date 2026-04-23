#![allow(dead_code)]

use super::super::common::extract_tool_subtitle;
use crate::chat::model::{EntryKind, TimelineEntry, ToolCallStatus};
use std::borrow::Cow;

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

const MAX_PREVIEW_LINES: usize = 2;

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

    let preview_source = preview_source_text(status, entry);
    let subtitle = extract_tool_subtitle(tool_name, &entry.details);

    Some(EntryDescriptor {
        status_visual,
        title: tool_name.clone(),
        subtitle: (!subtitle.is_empty()).then_some(subtitle),
        preview_lines: build_preview_lines(preview_source.as_ref()),
        result_mode: infer_result_mode(tool_name, preview_source.as_ref()),
        call_identity: Some(call_id.clone()),
    })
}

fn preview_source_text<'a>(status: &ToolCallStatus, entry: &'a TimelineEntry) -> Cow<'a, str> {
    if !matches!(status, ToolCallStatus::Running)
        && let Some(full_output) = completed_output_text(&entry.details)
    {
        return Cow::Owned(full_output);
    }

    Cow::Borrowed(&entry.body)
}

fn completed_output_text(details: &str) -> Option<String> {
    let mut lines: Vec<&str> = details.lines().collect();
    if !matches!(lines.last(), Some(line) if is_tool_metadata_line(line)) {
        return None;
    }

    lines.pop();
    let content = lines.join("\n").trim().to_owned();
    (!content.is_empty()).then_some(content)
}

fn is_tool_metadata_line(line: &str) -> bool {
    line.starts_with("batch_id=")
        && line.contains(" replay_index=")
        && line.contains(" normalized_claims=")
}

fn build_preview_lines(text: &str) -> Vec<String> {
    text.lines()
        .take(MAX_PREVIEW_LINES)
        .map(ToOwned::to_owned)
        .collect()
}

fn infer_result_mode(tool_name: &str, body: &str) -> ResultMode {
    let is_diff =
        matches!(tool_name, "edit_file") || body.starts_with("diff --git") || body.contains("\n@@");
    if is_diff {
        return ResultMode::Diff;
    }

    if body.contains("```") {
        return ResultMode::Code;
    }

    if body.contains("# ") || body.contains("\n- ") || body.starts_with("- ") {
        return ResultMode::Markdown;
    }

    ResultMode::Plain
}
