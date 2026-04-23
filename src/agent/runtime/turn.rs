use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};

use log::{debug, info, warn};
use serde_json::Value;

use crate::agent::protocol::AgentEvent;
use crate::agent::tools::scheduler::{ScheduledCall, schedule_batches};
use crate::agent::tools::{ContentPart, ToolKind, ToolRegistry, ToolResult};
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
    fn on_tool_call_started(&self, call_id: &str, tool_name: &str, kind: ToolKind, arguments: &str);
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
    fn on_tool_call_started(
        &self,
        call_id: &str,
        tool_name: &str,
        kind: ToolKind,
        arguments: &str,
    ) {
        let _ = self.tx.send(AgentEvent::ToolCallStarted {
            call_id: call_id.to_owned(),
            tool_name: tool_name.to_owned(),
            kind,
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
    fn on_tool_call_started(
        &self,
        _call_id: &str,
        _tool_name: &str,
        _kind: ToolKind,
        _arguments: &str,
    ) {
    }
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

enum PreparedToolAction {
    Immediate(ToolResult),
    Execute {
        args: Value,
        was_repaired: bool,
        signature: (String, String),
    },
}

struct PreparedToolCall {
    pending: PendingToolCall,
    action: PreparedToolAction,
}

struct ExecutedToolCall {
    pending: PendingToolCall,
    result: ToolResult,
    success_signature: Option<(String, String)>,
}

struct BatchExecution {
    executed_calls: Vec<ExecutedToolCall>,
    cancelled_before_launch_completed: bool,
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

                let prepared_calls = pending_calls
                    .into_iter()
                    .map(|pending| {
                        prepare_tool_call(
                            tools,
                            pending,
                            &previous_pass_successes,
                            &mut consecutive_failures,
                        )
                    })
                    .collect();
                let mut current_pass_successes: HashSet<(String, String)> = HashSet::new();

                for batch in schedule_batches(prepared_calls) {
                    if cancel_flag.load(Ordering::SeqCst) {
                        observer.on_turn_failed("cancelled");
                        return TurnOutcome::Cancelled;
                    }

                    if tool_iterations >= MAX_TOOL_ITERATIONS {
                        return TurnOutcome::FatalError("tool iteration limit exceeded".into());
                    }

                    let remaining_iterations = MAX_TOOL_ITERATIONS - tool_iterations;
                    let batch_len = batch.len().min(remaining_iterations);
                    let truncated_batch = batch_len < batch.len();
                    let executing_batch = batch.into_iter().take(batch_len).collect::<Vec<_>>();

                    let batch_execution =
                        execute_batch(executing_batch, tools, observer, cancel_flag);
                    for mut executed in batch_execution.executed_calls {
                        if let Some(signature) = executed.success_signature.clone() {
                            if executed.result.is_error {
                                executed.result = annotate_timeout_failure(
                                    executed.result,
                                    &executed.pending.name,
                                    &signature.1,
                                    &mut consecutive_timeout_failures,
                                );
                            } else {
                                consecutive_timeout_failures.remove(&signature);
                                current_pass_successes.insert(signature);
                            }
                        }

                        let output = executed.result.text_output();
                        observer.on_tool_call_completed(
                            &executed.pending.call_id,
                            &output,
                            executed.result.is_error,
                        );

                        messages.push(ProviderMessage::ToolResult {
                            tool_call_id: executed.pending.call_id,
                            output,
                            is_error: executed.result.is_error,
                        });
                        tool_iterations += 1;
                    }

                    if batch_execution.cancelled_before_launch_completed {
                        observer.on_turn_failed("cancelled");
                        return TurnOutcome::Cancelled;
                    }

                    if truncated_batch {
                        return TurnOutcome::FatalError("tool iteration limit exceeded".into());
                    }
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

fn prepare_tool_call(
    tools: &ToolRegistry,
    pending: PendingToolCall,
    previous_pass_successes: &HashSet<(String, String)>,
    consecutive_failures: &mut HashMap<(String, String), usize>,
) -> ScheduledCall<PreparedToolCall> {
    let cleaned_args = strip_thinking_tags(&pending.arguments);
    match parse_or_repair_json(&cleaned_args) {
        Ok(parsed) => {
            consecutive_failures.remove(&(pending.name.clone(), pending.arguments.clone()));
            let signature = (
                pending.name.clone(),
                canonical_tool_args(&parsed.value, &cleaned_args),
            );
            if previous_pass_successes.contains(&signature) {
                ScheduledCall {
                    item: PreparedToolCall {
                        pending,
                        action: PreparedToolAction::Immediate(ToolResult::failure(
                            REPEATED_SUCCESS_GUIDANCE,
                        )),
                    },
                    claims: Vec::new(),
                }
            } else {
                let claims = tools.resource_claims(&pending.name, &parsed.value);
                ScheduledCall {
                    item: PreparedToolCall {
                        pending,
                        action: PreparedToolAction::Execute {
                            args: parsed.value,
                            was_repaired: parsed.was_repaired,
                            signature,
                        },
                    },
                    claims,
                }
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

            let result = if *count >= 2 {
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
            };

            ScheduledCall {
                item: PreparedToolCall {
                    pending,
                    action: PreparedToolAction::Immediate(result),
                },
                claims: Vec::new(),
            }
        }
    }
}

fn execute_batch(
    batch: Vec<ScheduledCall<PreparedToolCall>>,
    tools: &ToolRegistry,
    observer: &dyn TurnObserver,
    cancel_flag: &Arc<AtomicBool>,
) -> BatchExecution {
    let batch_len = batch.len();
    let mut results = std::iter::repeat_with(|| None)
        .take(batch_len)
        .collect::<Vec<Option<ExecutedToolCall>>>();
    let mut spawned_count = 0usize;

    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::channel();

        for (index, scheduled) in batch.into_iter().enumerate() {
            if cancel_flag.load(Ordering::SeqCst) {
                break;
            }

            observer.on_tool_call_started(
                &scheduled.item.pending.call_id,
                &scheduled.item.pending.name,
                tool_kind_for_name(tools, &scheduled.item.pending.name),
                &scheduled.item.pending.arguments,
            );

            match scheduled.item.action {
                PreparedToolAction::Immediate(result) => {
                    results[index] = Some(ExecutedToolCall {
                        pending: scheduled.item.pending,
                        result,
                        success_signature: None,
                    });
                    spawned_count += 1;
                }
                PreparedToolAction::Execute {
                    args,
                    was_repaired,
                    signature,
                } => {
                    let tx = tx.clone();
                    let pending = scheduled.item.pending;
                    spawned_count += 1;
                    scope.spawn(move || {
                        let mut result = execute_tool_call(tools, &pending.name, args);
                        if was_repaired {
                            let output = result.text_output();
                            result.content = vec![ContentPart::text(format!(
                                "{}\n\n{}",
                                JSON_REPAIR_NOTE, output
                            ))];
                        }

                        let _ = tx.send((
                            index,
                            ExecutedToolCall {
                                pending,
                                result,
                                success_signature: Some(signature),
                            },
                        ));
                    });
                }
            }
        }

        drop(tx);
        for (index, result) in rx {
            results[index] = Some(result);
        }
    });

    let executed_calls = results
        .into_iter()
        .take(spawned_count)
        .map(|result| result.expect("batch execution should produce ordered results"))
        .collect();

    BatchExecution {
        executed_calls,
        cancelled_before_launch_completed: spawned_count < batch_len,
    }
}

fn tool_kind_for_name(tools: &ToolRegistry, tool_name: &str) -> ToolKind {
    match tools.get(tool_name) {
        Some(tool) => tool.kind(),
        None => {
            warn!("runtime: missing tool metadata for {tool_name}; defaulting ToolKind::Local");
            ToolKind::Local
        }
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
