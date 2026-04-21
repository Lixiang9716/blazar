use std::collections::{HashMap, HashSet};
use std::process::Command;

use crate::provider::ProviderMessage;

use super::{FunctionCall, ToolCall, ToolChoice};

pub(super) fn render_system_prompt(base: &str) -> String {
    match build_runtime_context_block() {
        Some(context) => format!("{base}\n\n{context}"),
        None => base.to_owned(),
    }
}

fn build_runtime_context_block() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let platform = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    let git_branch = run_git_command(&cwd, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|| "unknown".to_owned());
    let git_status = run_git_command(
        &cwd,
        &["status", "--short", "--branch", "--untracked-files=no"],
    );

    let mut block = vec![
        "## Runtime Context".to_owned(),
        format!("- Working directory: {}", cwd.display()),
        format!("- Platform: {platform}"),
        format!("- Git branch: {git_branch}"),
    ];

    if let Some(status) = git_status
        && !status.is_empty()
    {
        block.push("- Git status:".to_owned());
        block.push("```text".to_owned());
        block.push(status);
        block.push("```".to_owned());
    }

    Some(block.join("\n"))
}

fn run_git_command(cwd: &std::path::Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

pub(super) fn collect_tool_call_batch(
    messages: &[ProviderMessage],
    start: usize,
) -> (Vec<ToolCall>, usize) {
    let mut collected = Vec::new();
    let mut index = start;

    while index < messages.len() {
        match &messages[index] {
            ProviderMessage::ToolCall {
                id,
                name,
                arguments,
            } => {
                collected.push(ToolCall {
                    id: id.clone(),
                    call_type: "function".into(),
                    function: FunctionCall {
                        name: name.clone(),
                        arguments: arguments.clone(),
                    },
                });
                index += 1;
            }
            _ => break,
        }
    }

    (collected, index)
}

const MAX_CONTEXT_USER_TURNS: usize = 10;
const MAX_CONTEXT_PROVIDER_MESSAGES: usize = 80;

pub(super) fn truncate_provider_messages(messages: &[ProviderMessage]) -> Vec<ProviderMessage> {
    if messages.is_empty() {
        return Vec::new();
    }

    let user_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| match message {
            ProviderMessage::User { .. } => Some(index),
            _ => None,
        })
        .collect();

    let mut start = 0usize;
    if user_indices.len() > MAX_CONTEXT_USER_TURNS {
        start = user_indices[user_indices.len() - MAX_CONTEXT_USER_TURNS];
    }

    if messages.len().saturating_sub(start) > MAX_CONTEXT_PROVIDER_MESSAGES {
        let tail_start = messages.len() - MAX_CONTEXT_PROVIDER_MESSAGES;
        start = user_indices
            .iter()
            .copied()
            .find(|idx| *idx >= tail_start)
            .unwrap_or(tail_start)
            .max(start);
    }

    messages[start..].to_vec()
}

pub(super) fn determine_tool_choice(
    messages: &[ProviderMessage],
    has_tools: bool,
) -> Option<ToolChoice> {
    if !has_tools {
        return None;
    }
    if has_repeated_successful_tool_calls(messages) {
        Some(ToolChoice::None)
    } else {
        Some(ToolChoice::Auto)
    }
}

fn has_repeated_successful_tool_calls(messages: &[ProviderMessage]) -> bool {
    let mut pending_calls: HashMap<&str, (&str, &str)> = HashMap::new();
    let mut seen_successes: HashSet<(String, String, String)> = HashSet::new();

    for message in messages {
        match message {
            ProviderMessage::ToolCall {
                id,
                name,
                arguments,
            } => {
                pending_calls.insert(id.as_str(), (name.as_str(), arguments.as_str()));
            }
            ProviderMessage::ToolResult {
                tool_call_id,
                output,
                is_error: false,
            } => {
                if let Some((name, arguments)) = pending_calls.remove(tool_call_id.as_str()) {
                    let success = (name.to_string(), arguments.to_string(), output.clone());
                    if !seen_successes.insert(success) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }

    false
}
