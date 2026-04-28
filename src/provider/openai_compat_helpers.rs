use std::collections::{HashMap, HashSet};
use std::process::Command;

use super::{DeltaToolCall, FunctionCall, ProviderMessage, ToolCall, ToolChoice};

pub fn merge_tool_call_fragment(tool_calls: &mut Vec<ToolCall>, dtc: &DeltaToolCall) {
    let idx = dtc.index as usize;
    while tool_calls.len() <= idx {
        tool_calls.push(ToolCall {
            id: String::new(),
            call_type: "function".to_owned(),
            function: FunctionCall {
                name: String::new(),
                arguments: String::new(),
            },
        });
    }

    if let Some(ref id) = dtc.id {
        tool_calls[idx].id.clone_from(id);
    }
    if let Some(ref call_type) = dtc.call_type {
        tool_calls[idx].call_type.clone_from(call_type);
    }
    if let Some(ref function) = dtc.function {
        if let Some(ref name) = function.name {
            tool_calls[idx].function.name.clone_from(name);
        }
        if let Some(ref arguments) = function.arguments {
            tool_calls[idx].function.arguments.push_str(arguments);
        }
    }
}

pub(crate) fn render_system_prompt(base: &str) -> String {
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

pub(crate) fn run_git_command(cwd: &std::path::Path, args: &[&str]) -> Option<String> {
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

pub(crate) fn collect_tool_call_batch(
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

pub(crate) fn truncate_provider_messages(messages: &[ProviderMessage]) -> Vec<ProviderMessage> {
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

pub(crate) fn determine_tool_choice(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_system_prompt_appends_context() {
        let result = render_system_prompt("You are a helpful assistant.");
        // build_runtime_context_block always returns Some in a normal env
        // (current_dir succeeds), so the result should contain the base prompt
        // plus a Runtime Context section.
        assert!(result.starts_with("You are a helpful assistant."));
        assert!(result.contains("## Runtime Context"));
    }

    #[test]
    fn render_system_prompt_preserves_base_when_no_context() {
        // We can't easily force build_runtime_context_block to return None
        // in a normal test environment (current_dir always succeeds).
        // Instead, we verify the base prompt is always present.
        let base = "Base prompt only.";
        let result = render_system_prompt(base);
        assert!(result.contains(base));
    }

    #[test]
    fn truncate_empty_returns_empty() {
        let result = truncate_provider_messages(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn truncate_preserves_small_message_list() {
        let messages = vec![
            ProviderMessage::User {
                content: "hello".into(),
            },
            ProviderMessage::Assistant {
                content: "hi".into(),
            },
        ];
        let result = truncate_provider_messages(&messages);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn truncate_trims_excess_user_turns() {
        // Build a message list with more than MAX_CONTEXT_USER_TURNS user messages.
        let mut messages = Vec::new();
        for i in 0..MAX_CONTEXT_USER_TURNS + 5 {
            messages.push(ProviderMessage::User {
                content: format!("msg-{i}"),
            });
            messages.push(ProviderMessage::Assistant {
                content: format!("reply-{i}"),
            });
        }
        let result = truncate_provider_messages(&messages);
        let user_count = result
            .iter()
            .filter(|m| matches!(m, ProviderMessage::User { .. }))
            .count();
        assert!(
            user_count <= MAX_CONTEXT_USER_TURNS,
            "should have at most {MAX_CONTEXT_USER_TURNS} user turns, got {user_count}"
        );
    }

    #[test]
    fn truncate_respects_max_provider_messages() {
        // Build a list exceeding MAX_CONTEXT_PROVIDER_MESSAGES.
        let mut messages = Vec::new();
        for i in 0..MAX_CONTEXT_PROVIDER_MESSAGES + 20 {
            messages.push(ProviderMessage::User {
                content: format!("u-{i}"),
            });
        }
        let result = truncate_provider_messages(&messages);
        assert!(
            result.len() <= MAX_CONTEXT_PROVIDER_MESSAGES,
            "should have at most {} messages, got {}",
            MAX_CONTEXT_PROVIDER_MESSAGES,
            result.len()
        );
    }

    #[test]
    fn merge_tool_call_fragment_builds_incrementally() {
        let mut tool_calls = Vec::new();
        let delta = DeltaToolCall {
            index: 0,
            id: Some("call-1".into()),
            call_type: Some("function".into()),
            function: Some(super::super::DeltaFunction {
                name: Some("my_tool".into()),
                arguments: Some("{\"key\":".into()),
            }),
        };
        merge_tool_call_fragment(&mut tool_calls, &delta);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "my_tool");

        // Append more arguments
        let delta2 = DeltaToolCall {
            index: 0,
            id: None,
            call_type: None,
            function: Some(super::super::DeltaFunction {
                name: None,
                arguments: Some("\"val\"}".into()),
            }),
        };
        merge_tool_call_fragment(&mut tool_calls, &delta2);
        assert_eq!(tool_calls[0].function.arguments, "{\"key\":\"val\"}");
    }

    #[test]
    fn collect_tool_call_batch_gathers_consecutive_calls() {
        let messages = vec![
            ProviderMessage::ToolCall {
                id: "c1".into(),
                name: "tool_a".into(),
                arguments: "{}".into(),
            },
            ProviderMessage::ToolCall {
                id: "c2".into(),
                name: "tool_b".into(),
                arguments: "{}".into(),
            },
            ProviderMessage::User {
                content: "stop".into(),
            },
        ];
        let (batch, next_index) = collect_tool_call_batch(&messages, 0);
        assert_eq!(batch.len(), 2);
        assert_eq!(next_index, 2);
        assert_eq!(batch[0].function.name, "tool_a");
        assert_eq!(batch[1].function.name, "tool_b");
    }

    #[test]
    fn determine_tool_choice_returns_none_without_tools() {
        assert!(determine_tool_choice(&[], false).is_none());
    }

    #[test]
    fn determine_tool_choice_returns_auto_for_fresh_conversation() {
        let messages = vec![ProviderMessage::User {
            content: "hi".into(),
        }];
        let choice = determine_tool_choice(&messages, true);
        assert_eq!(choice, Some(ToolChoice::Auto));
    }

    #[test]
    fn truncate_triggers_provider_message_limit() {
        // Lines 143-150: build a list exceeding MAX_CONTEXT_PROVIDER_MESSAGES
        // with user messages spaced far apart to force the inner tail_start logic.
        let mut messages = Vec::new();
        // Add a few user messages at the start, then a huge block of tool results.
        for i in 0..3 {
            messages.push(ProviderMessage::User {
                content: format!("user-{i}"),
            });
        }
        // Fill with tool calls + results to exceed the 80 message limit.
        for i in 0..90 {
            messages.push(ProviderMessage::ToolCall {
                id: format!("tc-{i}"),
                name: "bash".into(),
                arguments: "{}".into(),
            });
            messages.push(ProviderMessage::ToolResult {
                tool_call_id: format!("tc-{i}"),
                output: "ok".into(),
                is_error: false,
            });
        }
        // Add final user turns at the end.
        for i in 0..3 {
            messages.push(ProviderMessage::User {
                content: format!("final-{i}"),
            });
        }
        let result = truncate_provider_messages(&messages);
        assert!(
            result.len() <= MAX_CONTEXT_PROVIDER_MESSAGES,
            "should respect provider message limit, got {}",
            result.len()
        );
        // The result should start at a User message boundary.
        assert!(
            matches!(&result[0], ProviderMessage::User { .. }),
            "truncated messages should start at a user boundary"
        );
    }

    #[test]
    fn run_git_command_returns_none_for_empty_output() {
        // Line 84: git command succeeds but returns empty output.
        let cwd = std::env::current_dir().unwrap();
        let result = run_git_command(&cwd, &["hash-object", "--stdin"]);
        // This won't succeed without stdin, so returns None via status check.
        // Use a command that succeeds with empty stdout:
        // Actually, let's use `git log --oneline -0` which outputs nothing.
        let _result = result; // suppress unused warning
        let result = run_git_command(&cwd, &["log", "--oneline", "-0"]);
        // On a valid repo, -0 means show 0 commits → empty output → None.
        assert!(result.is_none(), "empty git output should return None");
    }

    #[test]
    fn run_git_command_returns_some_for_valid_command() {
        let cwd = std::env::current_dir().unwrap();
        let result = run_git_command(&cwd, &["rev-parse", "--is-inside-work-tree"]);
        // In the blazar repo this should return "true"
        assert_eq!(result, Some("true".to_owned()));
    }

    #[test]
    fn run_git_command_returns_none_for_invalid_command() {
        let cwd = std::env::current_dir().unwrap();
        let result = run_git_command(&cwd, &["no-such-subcommand"]);
        assert!(result.is_none());
    }
}
