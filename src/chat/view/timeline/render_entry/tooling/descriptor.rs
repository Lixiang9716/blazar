#![allow(dead_code)]

use super::super::common::{extract_tool_subtitle, extract_tool_subtitle_from_details};
use crate::chat::app::turns::{tool_call_details_payload, tool_call_metadata_line};
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
}

const MAX_PREVIEW_LINES: usize = 2;

pub(crate) fn tool_descriptor(entry: &TimelineEntry) -> Option<EntryDescriptor> {
    let EntryKind::ToolCall {
        tool_name,
        arguments,
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
    let subtitle = if arguments.trim().is_empty() {
        extract_tool_subtitle_from_details(tool_name, &entry.details)
    } else {
        let extracted = extract_tool_subtitle(tool_name, arguments);
        if extracted.is_empty() {
            arguments.clone()
        } else {
            extracted
        }
    };

    Some(EntryDescriptor {
        status_visual,
        title: tool_name.clone(),
        subtitle: (!subtitle.is_empty()).then_some(subtitle),
        preview_lines: build_preview_lines(preview_source.as_ref()),
        result_mode: infer_result_mode(tool_name, preview_source.as_ref()),
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
    tool_call_metadata_line(details)?;

    let content = tool_call_details_payload(details).trim().to_owned();
    (!content.is_empty()).then_some(content)
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
