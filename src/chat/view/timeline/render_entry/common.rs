use super::*;
use crate::agent::tools::ToolKind;
use crate::chat::app::turns::tool_call_details_payload;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(super) fn marker_style_for(entry: &TimelineEntry, theme: &ChatTheme) -> Style {
    match (&entry.actor, &entry.kind) {
        (Actor::User, _) => theme.marker_response,
        (_, EntryKind::Warning) => theme.marker_warning,
        (_, EntryKind::Hint) => theme.marker_hint,
        (_, EntryKind::Thinking) => theme.marker_thinking,
        (_, EntryKind::ToolUse { .. } | EntryKind::ToolCall { .. } | EntryKind::Bash { .. }) => {
            theme.marker_tool
        }
        (_, EntryKind::CodeBlock { .. }) => theme.marker_tool,
        _ => theme.marker_response,
    }
}

/// Extract a short inline parameter summary from tool-call arguments.
/// Shows the most useful field — file path for read/write, command for bash.
pub(super) fn extract_tool_subtitle(tool_name: &str, arguments: &str) -> String {
    if arguments.trim().is_empty() {
        return String::new();
    }

    let val: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(_) => return "invalid args".to_owned(),
    };

    let key = match tool_name {
        "read_file" | "write_file" | "create_file" | "list_dir" => "path",
        "edit_file" => "path",
        "bash" | "shell" => "command",
        "grep" | "ripgrep" => "pattern",
        "search" | "find_files" => "query",
        _ => {
            // Fallback: try common keys in order
            for k in &["path", "file", "command", "query", "url"] {
                if let Some(s) = val.get(*k).and_then(|v| v.as_str()) {
                    return truncate_subtitle(s);
                }
            }
            return String::new();
        }
    };

    val.get(key)
        .and_then(|v| v.as_str())
        .map(truncate_subtitle)
        .unwrap_or_default()
}

fn truncate_subtitle(s: &str) -> String {
    truncate_display_width(s, 78)
}

pub(super) fn truncate_display_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_owned();
    }
    if max_width == 1 {
        return "…".to_owned();
    }

    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if used + width > max_width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += width;
    }
    out.push('…');
    out
}

pub(super) fn tool_badge(kind: ToolKind) -> Option<&'static str> {
    match kind {
        ToolKind::Agent { is_acp: true } => Some("(ACP)"),
        _ => None,
    }
}

pub(super) fn extract_tool_subtitle_from_details(tool_name: &str, details: &str) -> String {
    let arguments = tool_call_details_payload(details).trim();
    if !matches!(arguments.chars().next(), Some('{') | Some('[')) {
        return String::new();
    }

    extract_tool_subtitle(tool_name, arguments)
}
