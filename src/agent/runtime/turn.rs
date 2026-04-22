use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};

use log::{debug, info, warn};
use serde_json::Value;

use crate::agent::protocol::AgentEvent;
use crate::agent::tools::{ContentPart, ToolRegistry, ToolResult};
use crate::provider::{LlmProvider, ProviderEvent, ProviderMessage};

use super::json_repair::{
    canonical_tool_args, parse_or_repair_json, preview_text, strip_thinking_tags,
};
use super::{
    JSON_REPAIR_NOTE, MAX_TOOL_ITERATIONS, REPEATED_SUCCESS_GUIDANCE, REPEATED_TIMEOUT_GUIDANCE,
    TIMEOUT_NOTE,
};

// ── Turn observer ──────────────────────────────────────────────────
//
// Abstracts the event side-channel so the core turn logic can be reused
// both by the root agent (streaming events to the UI) and by sub-agent
// tools (which silently collect the final result).

/// Observer that receives lifecycle events during a turn.
///
/// The root runtime sends these to the UI via `Sender<AgentEvent>`.
/// Sub-agent tool invocations use [`SilentObserver`] which discards them.
pub(crate) trait TurnObserver {
    fn on_text_delta(&self, text: &str);
    fn on_thinking_delta(&self, text: &str);
    fn on_tool_call_started(&self, call_id: &str, tool_name: &str, arguments: &str);
    fn on_tool_call_completed(&self, call_id: &str, output: &str, is_error: bool);
    fn on_turn_failed(&self, error: &str);
}

/// Observer that forwards events to a `Sender<AgentEvent>` (UI channel).
pub(super) struct ChannelObserver<'a> {
    pub(super) tx: &'a Sender<AgentEvent>,
}

impl TurnObserver for ChannelObserver<'_> {
    fn on_text_delta(&self, text: &str) {
        let _ = self.tx.send(AgentEvent::TextDelta {
            text: text.to_owned(),
        });
    }
    fn on_thinking_delta(&self, text: &str) {
        let _ = self.tx.send(AgentEvent::ThinkingDelta {
            text: text.to_owned(),
        });
    }
    fn on_tool_call_started(&self, call_id: &str, tool_name: &str, arguments: &str) {
        let _ = self.tx.send(AgentEvent::ToolCallStarted {
            call_id: call_id.to_owned(),
            tool_name: tool_name.to_owned(),
            arguments: arguments.to_owned(),
        });
    }
    fn on_tool_call_completed(&self, call_id: &str, output: &str, is_error: bool) {
        let _ = self.tx.send(AgentEvent::ToolCallCompleted {
            call_id: call_id.to_owned(),
            output: output.to_owned(),
            is_error,
        });
    }
    fn on_turn_failed(&self, error: &str) {
        let _ = self.tx.send(AgentEvent::TurnFailed {
            error: error.to_owned(),
        });
    }
}

/// Observer that silently discards all events.
///
/// Used by sub-agent tool invocations where only the final text matters.
pub(crate) struct SilentObserver;

impl TurnObserver for SilentObserver {
    fn on_text_delta(&self, _text: &str) {}
    fn on_thinking_delta(&self, _text: &str) {}
    fn on_tool_call_started(&self, _call_id: &str, _tool_name: &str, _arguments: &str) {}
    fn on_tool_call_completed(&self, _call_id: &str, _output: &str, _is_error: bool) {}
    fn on_turn_failed(&self, _error: &str) {}
}

pub(crate) enum TurnOutcome {
    Complete,
    Cancelled,
    TransientError(String),
    FatalError(String),
}

pub(super) struct PendingToolCall {
    call_id: String,
    name: String,
    arguments: String,
}

pub(super) struct ProviderPass {
    pub(super) outcome: TurnOutcome,
    pub(super) assistant_text: String,
    pub(super) tool_calls: Vec<PendingToolCall>,
}

/// Classify whether an error is transient (network timeout, 429, 502/503).
pub(crate) fn is_transient_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("timeout")
        || lower.contains("429")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("connection")
        || lower.contains("rate limit")
}

/// Execute a single turn, including bounded tool-call re-entry.
///
/// This is the core agentic loop.  It is generic over [`TurnObserver`]
/// so the same logic serves both the root runtime (streaming to UI)
/// and sub-agent tool calls (silent).
pub(crate) fn execute_turn(
    messages: &mut Vec<ProviderMessage>,
    provider: &dyn LlmProvider,
    model: &str,
    tools: &ToolRegistry,
    observer: &dyn TurnObserver,
    cancel_flag: &Arc<AtomicBool>,
) -> TurnOutcome {
    let tool_specs = tools.specs();
    let mut tool_iterations = 0usize;
    // Track (tool_name, raw_args) → consecutive failure count for duplicate detection.
    let mut consecutive_failures: HashMap<(String, String), usize> = HashMap::new();
    let mut consecutive_timeout_failures: HashMap<(String, String), usize> = HashMap::new();
    let mut previous_pass_successes: HashSet<(String, String)> = HashSet::new();

    loop {
        if cancel_flag.load(Ordering::SeqCst) {
            observer.on_turn_failed("cancelled");
            return TurnOutcome::Cancelled;
        }

        let pass = stream_provider_pass(
            provider,
            model,
            messages,
            &tool_specs,
            observer,
            cancel_flag,
        );

        match pass.outcome {
            TurnOutcome::Complete => {
                if pass.tool_calls.is_empty() {
                    if !pass.assistant_text.is_empty() {
                        messages.push(ProviderMessage::Assistant {
                            content: pass.assistant_text,
                        });
                    }
                    return TurnOutcome::Complete;
                }

                if !pass.assistant_text.is_empty() {
                    messages.push(ProviderMessage::Assistant {
                        content: pass.assistant_text,
                    });
                }

                let pending_calls = pass.tool_calls;
                for pending in &pending_calls {
                    messages.push(ProviderMessage::ToolCall {
                        id: pending.call_id.clone(),
                        name: pending.name.clone(),
                        arguments: pending.arguments.clone(),
                    });
                }

                let mut current_pass_successes: HashSet<(String, String)> = HashSet::new();
                for pending in pending_calls {
                    if cancel_flag.load(Ordering::SeqCst) {
                        observer.on_turn_failed("cancelled");
                        return TurnOutcome::Cancelled;
                    }

                    if tool_iterations >= MAX_TOOL_ITERATIONS {
                        return TurnOutcome::FatalError("tool iteration limit exceeded".into());
                    }

                    observer.on_tool_call_started(
                        &pending.call_id,
                        &pending.name,
                        &pending.arguments,
                    );

                    let cleaned_args = strip_thinking_tags(&pending.arguments);
                    let result = match parse_or_repair_json(&cleaned_args) {
                        Ok(parsed) => {
                            // Successful parse — clear any tracked failure for this tool.
                            consecutive_failures
                                .remove(&(pending.name.clone(), pending.arguments.clone()));
                            let signature = (
                                pending.name.clone(),
                                canonical_tool_args(&parsed.value, &cleaned_args),
                            );
                            if previous_pass_successes.contains(&signature) {
                                ToolResult::failure(REPEATED_SUCCESS_GUIDANCE)
                            } else {
                                let mut result =
                                    execute_tool_call(tools, &pending.name, parsed.value);
                                if parsed.was_repaired {
                                    let output = result.text_output();
                                    result.content = vec![ContentPart::text(format!(
                                        "{}\n\n{}",
                                        JSON_REPAIR_NOTE, output
                                    ))];
                                }
                                if result.is_error {
                                    result = annotate_timeout_failure(
                                        result,
                                        &pending.name,
                                        &signature.1,
                                        &mut consecutive_timeout_failures,
                                    );
                                } else {
                                    consecutive_timeout_failures.remove(&signature);
                                    current_pass_successes.insert(signature);
                                }
                                result
                            }
                        }
                        Err(error) => {
                            let fail_key = (pending.name.clone(), pending.arguments.clone());
                            let count = consecutive_failures.entry(fail_key).or_insert(0);
                            *count += 1;

                            warn!(
                                "runtime: invalid tool arguments for {}: {error}\n  raw: {}",
                                pending.name,
                                preview_text(&pending.arguments, 200)
                            );

                            if *count >= 2 {
                                ToolResult::failure(
                                    "REPEATED JSON ERROR: identical malformed arguments sent twice. \
                                     RULES: 1) All double quotes inside string values MUST be escaped as \\\". \
                                     2) Newlines inside strings MUST be \\n, not literal newlines. \
                                     3) For code containing quotes, use single quotes or escape them. \
                                     You MUST fix the JSON and retry now."
                                        .to_string(),
                                )
                            } else {
                                ToolResult::failure(format!(
                                    "JSON PARSE ERROR in tool arguments: {error}\n\
                                     Fix: ensure all double quotes inside string values are escaped \
                                     as \\\", and newlines are \\n. Then retry this tool call."
                                ))
                            }
                        }
                    };

                    let output = result.text_output();
                    observer.on_tool_call_completed(&pending.call_id, &output, result.is_error);

                    messages.push(ProviderMessage::ToolResult {
                        tool_call_id: pending.call_id,
                        output,
                        is_error: result.is_error,
                    });
                    tool_iterations += 1;
                }
                previous_pass_successes = current_pass_successes;
            }
            TurnOutcome::Cancelled => return TurnOutcome::Cancelled,
            TurnOutcome::TransientError(err) => {
                if tool_iterations > 0 {
                    return TurnOutcome::FatalError(err);
                }
                return TurnOutcome::TransientError(err);
            }
            TurnOutcome::FatalError(err) => return TurnOutcome::FatalError(err),
        }
    }
}

fn execute_tool_call(tools: &ToolRegistry, name: &str, args: Value) -> ToolResult {
    match tools.execute(name, args) {
        Ok(result) => result,
        Err(error) => ToolResult::failure(error),
    }
}

fn annotate_timeout_failure(
    mut result: ToolResult,
    tool_name: &str,
    canonical_args: &str,
    failures: &mut HashMap<(String, String), usize>,
) -> ToolResult {
    let key = (tool_name.to_string(), canonical_args.to_string());
    let output = result.text_output();
    if !is_timeout_output(&output) {
        failures.remove(&key);
        return result;
    }

    let count = failures
        .entry(key)
        .and_modify(|current| *current += 1)
        .or_insert(1);
    result.content = vec![ContentPart::text(format!("{}\n\n{}", output, TIMEOUT_NOTE))];
    if *count >= 2 {
        let output = result.text_output();
        result.content = vec![ContentPart::text(format!(
            "{}\n\n{}",
            output, REPEATED_TIMEOUT_GUIDANCE
        ))];
    }
    result
}

fn is_timeout_output(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    lower.contains("timed out") || lower.contains("timeout")
}

pub(super) fn stream_provider_pass(
    provider: &dyn LlmProvider,
    model: &str,
    messages: &[ProviderMessage],
    tool_specs: &[crate::agent::tools::ToolSpec],
    observer: &dyn TurnObserver,
    cancel_flag: &Arc<AtomicBool>,
) -> ProviderPass {
    let (chunk_tx, chunk_rx) = mpsc::channel::<ProviderEvent>();
    let mut pass = ProviderPass {
        outcome: TurnOutcome::Complete,
        assistant_text: String::new(),
        tool_calls: Vec::new(),
    };

    std::thread::scope(|scope| {
        scope.spawn(|| provider.stream_turn(model, messages, tool_specs, chunk_tx));

        for event in &chunk_rx {
            if cancel_flag.load(Ordering::SeqCst) {
                info!("stream_provider_pass: cancel flag observed, stopping relay");
                observer.on_turn_failed("cancelled");
                pass.outcome = TurnOutcome::Cancelled;
                return;
            }

            match event {
                ProviderEvent::TextDelta(text) => {
                    pass.assistant_text.push_str(&text);
                    observer.on_text_delta(&text);
                }
                ProviderEvent::ThinkingDelta(text) => {
                    observer.on_thinking_delta(&text);
                }
                ProviderEvent::ToolCall {
                    call_id,
                    name,
                    arguments,
                } => {
                    pass.tool_calls.push(PendingToolCall {
                        call_id,
                        name,
                        arguments,
                    });
                }
                ProviderEvent::TurnComplete => {
                    debug!("stream_provider_pass: provider sent TurnComplete");
                    break;
                }
                ProviderEvent::Error(err) => {
                    warn!("stream_provider_pass: provider error: {err}");
                    pass.outcome = if is_transient_error(&err) {
                        TurnOutcome::TransientError(err)
                    } else {
                        TurnOutcome::FatalError(err)
                    };
                    return;
                }
            }
        }
    });

    pass
}
