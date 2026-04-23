use std::collections::{HashMap, HashSet};

use log::warn;

use crate::agent::capability::{
    CapabilityClaim, CapabilityInput, CapabilityResult, ConflictPolicy,
};
use crate::agent::tools::ToolRegistry;

use super::REPEATED_SUCCESS_GUIDANCE;
use super::json_repair::{
    canonical_tool_args, parse_or_repair_json, preview_text, strip_thinking_tags,
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

            let input = CapabilityInput::new(parsed.value);
            let signature = (
                pending.name.clone(),
                canonical_tool_args(&input.arguments, &cleaned_args),
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
                     as \\\", and newlines are \\n. Then retry this tool call."
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
    fn scheduler_contract_matrix_is_stable_for_conflict_pairs() {
        let cases = vec![
            (claim_ro("fs:a"), claim_ro("fs:a"), false),
            (claim_ro("fs:a"), claim_rw("fs:a"), true),
            (claim_rw("fs:a"), claim_rw("fs:a"), true),
            (claim_ex("process:bash"), claim_ro("fs:a"), true),
        ];

        for (left, right, expected_conflict) in cases {
            let actual = ConflictPolicy::from_claims(&[left], &[right]).is_conflicting();
            assert_eq!(actual, expected_conflict);
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
