use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use log::warn;

use crate::agent::capability::{
    CapabilityContentPart, CapabilityInput, CapabilityKind, CapabilityResult,
};
use crate::agent::tools::{ToolKind, ToolRegistry};

use super::JSON_REPAIR_NOTE;
use super::events::TurnObserver;
use super::scheduler::{PendingToolCall, PlannedToolAction, PlannedToolCall, ScheduledCall};

pub(super) struct ExecutedToolCall {
    pub(super) pending: PendingToolCall,
    pub(super) result: CapabilityResult,
    pub(super) success_signature: Option<(String, String)>,
}

pub(super) struct BatchExecution {
    pub(super) executed_calls: Vec<ExecutedToolCall>,
    pub(super) cancelled_before_launch_completed: bool,
}

pub(super) fn execute_batch(
    batch: Vec<ScheduledCall<PlannedToolCall>>,
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
                PlannedToolAction::Immediate(result) => {
                    results[index] = Some(ExecutedToolCall {
                        pending: scheduled.item.pending,
                        result,
                        success_signature: None,
                    });
                    spawned_count += 1;
                }
                PlannedToolAction::Execute {
                    input,
                    was_repaired,
                    signature,
                } => {
                    let tx = tx.clone();
                    let pending = scheduled.item.pending;
                    spawned_count += 1;
                    scope.spawn(move || {
                        let mut result = execute_capability_call(tools, &pending.name, input);
                        if was_repaired {
                            let output = result.text_output();
                            result.content = vec![CapabilityContentPart::text(format!(
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

fn execute_capability_call(
    tools: &ToolRegistry,
    name: &str,
    input: CapabilityInput,
) -> CapabilityResult {
    match tools.execute(name, input.arguments) {
        Ok(result) => result.into_capability_result(),
        Err(error) => CapabilityResult::failure(error),
    }
}

fn tool_kind_for_name(tools: &ToolRegistry, tool_name: &str) -> ToolKind {
    match tools.capability_handle(tool_name) {
        Some(handle) => match handle.kind {
            CapabilityKind::Local => ToolKind::Local,
            CapabilityKind::Agent { is_acp } => ToolKind::Agent { is_acp },
        },
        None => {
            warn!("runtime: missing tool metadata for {tool_name}; defaulting ToolKind::Local");
            ToolKind::Local
        }
    }
}
