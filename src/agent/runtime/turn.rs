use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use log::{debug, info, warn};

use crate::agent::capability::{CapabilityContentPart, CapabilityResult};
use crate::agent::tools::ToolRegistry;
use crate::provider::{LlmProvider, ProviderEvent, ProviderMessage};

pub(crate) use super::events::{ChannelObserver, SilentObserver, TurnObserver};
use super::executor::execute_batch;
use super::scheduler::{PendingToolCall, plan_tool_call, schedule_batches};
use super::{MAX_TOOL_ITERATIONS, REPEATED_TIMEOUT_GUIDANCE, RuntimeErrorKind, TIMEOUT_NOTE};

pub(crate) enum TurnOutcome {
    Complete,
    Cancelled,
    TransientError(String),
    FatalError {
        kind: RuntimeErrorKind,
        error: String,
    },
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
            observer.on_turn_failed(RuntimeErrorKind::Cancelled, "cancelled");
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

                let planned_calls = pending_calls
                    .into_iter()
                    .map(|pending| {
                        plan_tool_call(
                            tools,
                            pending,
                            &previous_pass_successes,
                            &mut consecutive_failures,
                        )
                    })
                    .collect();
                let mut current_pass_successes: HashSet<(String, String)> = HashSet::new();

                for (batch_index, batch) in schedule_batches(planned_calls).into_iter().enumerate()
                {
                    if cancel_flag.load(Ordering::SeqCst) {
                        observer.on_turn_failed(RuntimeErrorKind::Cancelled, "cancelled");
                        return TurnOutcome::Cancelled;
                    }

                    if tool_iterations >= MAX_TOOL_ITERATIONS {
                        return TurnOutcome::FatalError {
                            kind: RuntimeErrorKind::ToolExecution,
                            error: "tool iteration limit exceeded".into(),
                        };
                    }

                    let remaining_iterations = MAX_TOOL_ITERATIONS - tool_iterations;
                    let batch_len = batch.len().min(remaining_iterations);
                    let truncated_batch = batch_len < batch.len();
                    let executing_batch = batch.into_iter().take(batch_len).collect::<Vec<_>>();

                    let batch_id = u32::try_from(batch_index).unwrap_or(u32::MAX);
                    let batch_execution =
                        execute_batch(executing_batch, batch_id, tools, observer, cancel_flag);
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
                        observer.on_turn_failed(RuntimeErrorKind::Cancelled, "cancelled");
                        return TurnOutcome::Cancelled;
                    }

                    if truncated_batch {
                        return TurnOutcome::FatalError {
                            kind: RuntimeErrorKind::ToolExecution,
                            error: "tool iteration limit exceeded".into(),
                        };
                    }
                }
                previous_pass_successes = current_pass_successes;
            }
            TurnOutcome::Cancelled => return TurnOutcome::Cancelled,
            TurnOutcome::TransientError(err) => {
                if tool_iterations > 0 {
                    return TurnOutcome::FatalError {
                        kind: RuntimeErrorKind::ProviderTransient,
                        error: err,
                    };
                }
                return TurnOutcome::TransientError(err);
            }
            TurnOutcome::FatalError { kind, error } => {
                return TurnOutcome::FatalError { kind, error };
            }
        }
    }
}

fn annotate_timeout_failure(
    mut result: CapabilityResult,
    tool_name: &str,
    canonical_args: &str,
    failures: &mut HashMap<(String, String), usize>,
) -> CapabilityResult {
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
    result.content = vec![CapabilityContentPart::text(format!(
        "{}\n\n{}",
        output, TIMEOUT_NOTE
    ))];
    if *count >= 2 {
        let output = result.text_output();
        result.content = vec![CapabilityContentPart::text(format!(
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
                observer.on_turn_failed(RuntimeErrorKind::Cancelled, "cancelled");
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
                        TurnOutcome::FatalError {
                            kind: RuntimeErrorKind::ProviderFatal,
                            error: err,
                        }
                    };
                    return;
                }
            }
        }
    });

    pass
}
