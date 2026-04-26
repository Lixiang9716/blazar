use std::collections::{HashMap, HashSet};

use log::warn;

use crate::agent::capability::{
    CapabilityClaim, CapabilityInput, CapabilityResult, ConflictPolicy,
};
use crate::agent::tools::ToolRegistry;

use super::REPEATED_SUCCESS_GUIDANCE;
use super::json_repair::{
    canonical_tool_args, parse_or_repair_json, preview_text, repair_invalid_dollar_escapes,
    repair_truncated_json_closure, repair_unescaped_inner_quotes, strip_thinking_tags,
};

pub(super) struct PendingToolCall {
    pub(super) call_id: String,
    pub(super) name: String,
    pub(super) arguments: String,
}

pub(super) enum PlannedToolAction {
    Immediate(CapabilityResult),
    Execute {
        input: CapabilityInput,
        was_repaired: bool,
        signature: (String, String),
    },
}

pub(super) struct PlannedToolCall {
    pub(super) pending: PendingToolCall,
    pub(super) action: PlannedToolAction,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ScheduledCall<T> {
    pub(super) item: T,
    pub(super) claims: Vec<CapabilityClaim>,
}

pub(super) fn plan_tool_call(
    tools: &ToolRegistry,
    pending: PendingToolCall,
    previous_pass_successes: &HashSet<(String, String)>,
    consecutive_failures: &mut HashMap<(String, String), usize>,
) -> ScheduledCall<PlannedToolCall> {
    let cleaned_args = strip_thinking_tags(&pending.arguments);
    match parse_or_repair_json(&cleaned_args) {
        Ok(parsed) => {
            consecutive_failures.remove(&(pending.name.clone(), pending.arguments.clone()));
            schedule_parsed_call(
                tools,
                pending,
                parsed.value,
                parsed.was_repaired,
                &cleaned_args,
                previous_pass_successes,
            )
        }
        Err(error) => {
            if pending.name == "bash"
                && let Some((repaired, repair_kind)) = repair_bash_arguments(&cleaned_args)
                && let Ok(value) = serde_json::from_str::<serde_json::Value>(&repaired)
            {
                warn!(
                    "runtime: repaired {repair_kind} in bash arguments\n  raw: {}",
                    preview_text(&pending.arguments, 200)
                );
                consecutive_failures.remove(&(pending.name.clone(), pending.arguments.clone()));
                return schedule_parsed_call(
                    tools,
                    pending,
                    value,
                    true,
                    &cleaned_args,
                    previous_pass_successes,
                );
            }

            let fail_key = (pending.name.clone(), pending.arguments.clone());
            let count = consecutive_failures.entry(fail_key).or_insert(0);
            *count += 1;

            warn!(
                "runtime: invalid tool arguments for {}: {error}\n  raw: {}",
                pending.name,
                preview_text(&pending.arguments, 200)
            );

            let result = if *count >= 2 {
                CapabilityResult::failure(
                    "REPEATED JSON ERROR: identical malformed arguments sent twice. \
                     RULES: 1) All double quotes inside string values MUST be escaped as \\\". \
                     2) Newlines inside strings MUST be \\n, not literal newlines. \
                     3) For code containing quotes, use single quotes or escape them. \
                     You MUST fix the JSON and retry now."
                        .to_string(),
                )
            } else {
                CapabilityResult::failure(format!(
                    "JSON PARSE ERROR in tool arguments: {error}\n\
                     Fix: ensure all double quotes inside string values are escaped \
                     as \\\", newlines are \\n, and JSON containers are fully closed. \
                     For bash commands, keep shell variables as $i / $(...) instead of \\\\$i / \\\\$(...). \
                     If multiple JSON objects are present, send exactly one JSON object. \
                     Then retry this tool call."
                ))
            };

            ScheduledCall {
                item: PlannedToolCall {
                    pending,
                    action: PlannedToolAction::Immediate(result),
                },
                claims: Vec::new(),
            }
        }
    }
}

fn repair_bash_arguments(raw: &str) -> Option<(String, &'static str)> {
    if let Some(repaired_quotes) = repair_unescaped_inner_quotes(raw) {
        if serde_json::from_str::<serde_json::Value>(&repaired_quotes).is_ok() {
            return Some((repaired_quotes, "unescaped quotes"));
        }
        if let Some(repaired_combo) = repair_invalid_dollar_escapes(&repaired_quotes)
            && serde_json::from_str::<serde_json::Value>(&repaired_combo).is_ok()
        {
            return Some((repaired_combo, "unescaped quotes + invalid dollar escapes"));
        }
        if let Some(repaired_combo) = repair_truncated_json_closure(&repaired_quotes)
            && serde_json::from_str::<serde_json::Value>(&repaired_combo).is_ok()
        {
            return Some((repaired_combo, "unescaped quotes + truncated payload"));
        }
        if let Some(repaired_dollars) = repair_invalid_dollar_escapes(&repaired_quotes)
            && let Some(repaired_full) = repair_truncated_json_closure(&repaired_dollars)
            && serde_json::from_str::<serde_json::Value>(&repaired_full).is_ok()
        {
            return Some((
                repaired_full,
                "unescaped quotes + invalid dollar escapes + truncated payload",
            ));
        }
    }

    if let Some(repaired_dollars) = repair_invalid_dollar_escapes(raw) {
        if serde_json::from_str::<serde_json::Value>(&repaired_dollars).is_ok() {
            return Some((repaired_dollars, "invalid dollar escapes"));
        }
        if let Some(repaired_combo) = repair_unescaped_inner_quotes(&repaired_dollars)
            && serde_json::from_str::<serde_json::Value>(&repaired_combo).is_ok()
        {
            return Some((repaired_combo, "invalid dollar escapes + unescaped quotes"));
        }
        if let Some(repaired_combo) = repair_truncated_json_closure(&repaired_dollars)
            && serde_json::from_str::<serde_json::Value>(&repaired_combo).is_ok()
        {
            return Some((repaired_combo, "invalid dollar escapes + truncated payload"));
        }
        if let Some(repaired_quotes) = repair_unescaped_inner_quotes(&repaired_dollars)
            && let Some(repaired_full) = repair_truncated_json_closure(&repaired_quotes)
            && serde_json::from_str::<serde_json::Value>(&repaired_full).is_ok()
        {
            return Some((
                repaired_full,
                "invalid dollar escapes + unescaped quotes + truncated payload",
            ));
        }
    }

    if let Some(repaired_truncated) = repair_truncated_json_closure(raw)
        && serde_json::from_str::<serde_json::Value>(&repaired_truncated).is_ok()
    {
        return Some((repaired_truncated, "truncated payload"));
    }

    None
}

fn schedule_parsed_call(
    tools: &ToolRegistry,
    pending: PendingToolCall,
    parsed_value: serde_json::Value,
    was_repaired: bool,
    signature_fallback: &str,
    previous_pass_successes: &HashSet<(String, String)>,
) -> ScheduledCall<PlannedToolCall> {
    let input = CapabilityInput::new(parsed_value);
    let signature = (
        pending.name.clone(),
        canonical_tool_args(&input.arguments, signature_fallback),
    );

    if previous_pass_successes.contains(&signature) {
        ScheduledCall {
            item: PlannedToolCall {
                pending,
                action: PlannedToolAction::Immediate(CapabilityResult::failure(
                    REPEATED_SUCCESS_GUIDANCE,
                )),
            },
            claims: Vec::new(),
        }
    } else {
        let claims = tools
            .resource_claims(&pending.name, &input.arguments)
            .into_iter()
            .map(Into::into)
            .collect();

        ScheduledCall {
            item: PlannedToolCall {
                pending,
                action: PlannedToolAction::Execute {
                    input,
                    was_repaired,
                    signature,
                },
            },
            claims,
        }
    }
}

pub(super) fn schedule_batches<T>(calls: Vec<ScheduledCall<T>>) -> Vec<Vec<ScheduledCall<T>>> {
    let mut batches = Vec::new();
    let mut current_batch = Vec::new();

    for call in calls {
        if current_batch.is_empty() || !batch_conflicts(&current_batch, &call.claims) {
            current_batch.push(call);
        } else {
            batches.push(current_batch);
            current_batch = vec![call];
        }
    }

    if !current_batch.is_empty() {
        batches.push(current_batch);
    }

    batches
}

fn batch_conflicts<T>(batch: &[ScheduledCall<T>], claims: &[CapabilityClaim]) -> bool {
    if batch.is_empty() {
        return false;
    }

    batch.iter().any(|scheduled_call| {
        ConflictPolicy::from_claims(&scheduled_call.claims, claims).is_conflicting()
    })
}

#[cfg(test)]
mod tests {
    use super::{ScheduledCall, schedule_batches};
    use crate::agent::capability::{CapabilityAccess, CapabilityClaim, ConflictPolicy};

    fn call(id: &'static str, claims: Vec<CapabilityClaim>) -> ScheduledCall<&'static str> {
        ScheduledCall { item: id, claims }
    }

    fn claim_ro(resource: &str) -> CapabilityClaim {
        CapabilityClaim {
            resource: resource.into(),
            access: CapabilityAccess::ReadOnly,
        }
    }

    fn claim_rw(resource: &str) -> CapabilityClaim {
        CapabilityClaim {
            resource: resource.into(),
            access: CapabilityAccess::ReadWrite,
        }
    }

    fn claim_ex(resource: &str) -> CapabilityClaim {
        CapabilityClaim {
            resource: resource.into(),
            access: CapabilityAccess::Exclusive,
        }
    }

    #[test]
    fn scheduler_contract_matrix_is_stable_for_claim_set_pairs() {
        let cases = vec![
            (
                "ro/ro same resource",
                vec![claim_ro("fs:a")],
                vec![claim_ro("fs:a")],
                false,
            ),
            (
                "ro/rw same resource",
                vec![claim_ro("fs:a")],
                vec![claim_rw("fs:a")],
                true,
            ),
            (
                "rw/ro same resource",
                vec![claim_rw("fs:a")],
                vec![claim_ro("fs:a")],
                true,
            ),
            (
                "rw/rw same resource",
                vec![claim_rw("fs:a")],
                vec![claim_rw("fs:a")],
                true,
            ),
            (
                "rw/rw different resources",
                vec![claim_rw("fs:a")],
                vec![claim_rw("fs:b")],
                false,
            ),
            (
                "ex/ro different resources",
                vec![claim_ex("process:bash")],
                vec![claim_ro("fs:a")],
                true,
            ),
            (
                "ex/ex same resource",
                vec![claim_ex("process:bash")],
                vec![claim_ex("process:bash")],
                true,
            ),
            (
                "multi-claim without overlap conflicts",
                vec![claim_ro("fs:a"), claim_rw("fs:b")],
                vec![claim_ro("fs:a"), claim_ro("fs:c")],
                false,
            ),
            (
                "multi-claim with overlapping rw conflicts",
                vec![claim_ro("fs:a"), claim_rw("fs:b")],
                vec![claim_ro("fs:a"), claim_rw("fs:b")],
                true,
            ),
        ];

        for (name, left, right, expected_conflict) in cases {
            let actual = ConflictPolicy::from_claims(&left, &right).is_conflicting();
            assert_eq!(
                actual, expected_conflict,
                "unexpected conflict policy for {name}"
            );
        }
    }

    #[test]
    fn scheduler_batches_shared_read_only_claims_together() {
        let batches = schedule_batches(vec![
            call(
                "read-a",
                vec![CapabilityClaim {
                    resource: "fs:src/main.rs".into(),
                    access: CapabilityAccess::ReadOnly,
                }],
            ),
            call(
                "read-b",
                vec![CapabilityClaim {
                    resource: "fs:src/main.rs".into(),
                    access: CapabilityAccess::ReadOnly,
                }],
            ),
            call(
                "write-c",
                vec![CapabilityClaim {
                    resource: "fs:src/main.rs".into(),
                    access: CapabilityAccess::ReadWrite,
                }],
            ),
        ]);

        assert_eq!(batches.len(), 2);
        assert_eq!(
            batches[0]
                .iter()
                .map(|scheduled| scheduled.item)
                .collect::<Vec<_>>(),
            vec!["read-a", "read-b"]
        );
        assert_eq!(
            batches[1]
                .iter()
                .map(|scheduled| scheduled.item)
                .collect::<Vec<_>>(),
            vec!["write-c"]
        );
    }

    #[test]
    fn scheduler_serializes_read_write_conflicts_on_same_resource() {
        let batches = schedule_batches(vec![
            call(
                "read-a",
                vec![CapabilityClaim {
                    resource: "fs:src/lib.rs".into(),
                    access: CapabilityAccess::ReadOnly,
                }],
            ),
            call(
                "write-b",
                vec![CapabilityClaim {
                    resource: "fs:src/lib.rs".into(),
                    access: CapabilityAccess::ReadWrite,
                }],
            ),
            call(
                "read-c",
                vec![CapabilityClaim {
                    resource: "fs:src/lib.rs".into(),
                    access: CapabilityAccess::ReadOnly,
                }],
            ),
        ]);

        assert_eq!(batches.len(), 3);
        assert_eq!(
            batches
                .iter()
                .map(|batch| batch[0].item)
                .collect::<Vec<&str>>(),
            vec!["read-a", "write-b", "read-c"]
        );
    }

    #[test]
    fn scheduler_batches_unrelated_reads_with_conflicting_writes() {
        let batches = schedule_batches(vec![
            call(
                "read-a",
                vec![CapabilityClaim {
                    resource: "fs:src/lib.rs".into(),
                    access: CapabilityAccess::ReadOnly,
                }],
            ),
            call(
                "write-b",
                vec![CapabilityClaim {
                    resource: "fs:src/lib.rs".into(),
                    access: CapabilityAccess::ReadWrite,
                }],
            ),
            call(
                "read-c",
                vec![CapabilityClaim {
                    resource: "fs:src/other.rs".into(),
                    access: CapabilityAccess::ReadOnly,
                }],
            ),
        ]);

        assert_eq!(batches.len(), 2);
        assert_eq!(
            batches[0]
                .iter()
                .map(|scheduled| scheduled.item)
                .collect::<Vec<_>>(),
            vec!["read-a"]
        );
        assert_eq!(
            batches[1]
                .iter()
                .map(|scheduled| scheduled.item)
                .collect::<Vec<_>>(),
            vec!["write-b", "read-c"]
        );
    }

    #[test]
    fn scheduler_treats_exclusive_claims_as_global_conflicts() {
        let batches = schedule_batches(vec![
            call(
                "read-a",
                vec![CapabilityClaim {
                    resource: "fs:src/lib.rs".into(),
                    access: CapabilityAccess::ReadOnly,
                }],
            ),
            call(
                "exclusive-bash",
                vec![CapabilityClaim {
                    resource: "process:bash".into(),
                    access: CapabilityAccess::Exclusive,
                }],
            ),
            call(
                "read-c",
                vec![CapabilityClaim {
                    resource: "fs:src/main.rs".into(),
                    access: CapabilityAccess::ReadOnly,
                }],
            ),
        ]);

        assert_eq!(batches.len(), 3);
        assert_eq!(
            batches
                .iter()
                .map(|batch| batch[0].item)
                .collect::<Vec<&str>>(),
            vec!["read-a", "exclusive-bash", "read-c"]
        );
    }
}
