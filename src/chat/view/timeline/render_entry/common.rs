use super::*;

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

/// Extract a short subtitle from tool-call arguments (stored in `details`).
/// Shows the most useful field — file path for read/write, command for bash.
pub(super) fn extract_tool_subtitle(tool_name: &str, details: &str) -> String {
    let val: serde_json::Value = match serde_json::from_str(details) {
        Ok(v) => v,
        Err(_) => return String::new(),
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
    if s.len() > 80 {
        format!("{}…", &s[..77])
    } else {
        s.to_owned()
    }
}
