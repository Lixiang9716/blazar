use std::collections::{HashMap, HashSet};

use jsonschema::JSONSchema;
use log::warn;

use super::events::{
    emit_tool_args_fim_correction_failed, emit_tool_args_fim_correction_requested,
    emit_tool_args_fim_correction_succeeded,
};
use crate::agent::capability::{
    CapabilityClaim, CapabilityInput, CapabilityResult, ConflictPolicy,
};
use crate::agent::tools::ToolRegistry;

use super::REPEATED_SUCCESS_GUIDANCE;
use super::json_repair::{
    canonical_tool_args, parse_error_category, parse_json_strict, parse_or_repair_json,
    preview_text, repair_truncated_json_closure, repair_unescaped_inner_quotes,
    strip_thinking_tags,
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

struct ToolArgsFailureDetail {
    category: &'static str,
    detail: String,
}

pub(super) fn plan_tool_call<F>(
    tools: &ToolRegistry,
    pending: PendingToolCall,
    previous_pass_successes: &HashSet<(String, String)>,
    consecutive_failures: &mut HashMap<(String, String), usize>,
    request_tool_args_correction: &mut F,
) -> ScheduledCall<PlannedToolCall>
where
    F: FnMut(&PendingToolCall, &str, &str) -> Option<String>,
{
    let cleaned_args = strip_thinking_tags(&pending.arguments);
    let fail_key = tool_args_failure_key(&pending.name, &cleaned_args);
    match parse_or_repair_json(&cleaned_args) {
        Ok(parsed) => {
            consecutive_failures.remove(&fail_key);
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
            let primary_error_category = parse_error_category(&error);
            let mut correction_failure: Option<ToolArgsFailureDetail> = None;

            if pending.name == "bash"
                && let Some((repaired, repair_kind)) = repair_bash_arguments(&cleaned_args)
                && let Ok(value) = serde_json::from_str::<serde_json::Value>(&repaired)
            {
                warn!(
                    "runtime: repaired {repair_kind} in bash arguments\n  raw: {}",
                    preview_text(&pending.arguments, 200)
                );
                consecutive_failures.remove(&fail_key);
                return schedule_parsed_call(
                    tools,
                    pending,
                    value,
                    true,
                    &cleaned_args,
                    previous_pass_successes,
                );
            }

            if !consecutive_failures.contains_key(&fail_key) {
                emit_tool_args_fim_correction_requested(
                    &pending.call_id,
                    &pending.name,
                    primary_error_category,
                );

                if let Some(corrected) =
                    request_tool_args_correction(&pending, &cleaned_args, &error.to_string())
                {
                    match parse_json_strict(&corrected) {
                        Ok(value) => {
                            match validate_fim_corrected_args(tools, &pending.name, &value) {
                                Ok(()) => {
                                    emit_tool_args_fim_correction_succeeded(
                                        &pending.call_id,
                                        &pending.name,
                                        primary_error_category,
                                    );
                                    consecutive_failures.remove(&fail_key);
                                    return schedule_parsed_call(
                                        tools,
                                        pending,
                                        value,
                                        true,
                                        &corrected,
                                        previous_pass_successes,
                                    );
                                }
                                Err(schema_error) => {
                                    emit_tool_args_fim_correction_failed(
                                        &pending.call_id,
                                        &pending.name,
                                        "schema_validation",
                                    );
                                    correction_failure = Some(ToolArgsFailureDetail {
                                        category: "schema_validation",
                                        detail: schema_error,
                                    });
                                }
                            }
                        }
                        Err(corrected_error) => {
                            emit_tool_args_fim_correction_failed(
                                &pending.call_id,
                                &pending.name,
                                parse_error_category(&corrected_error),
                            );
                            correction_failure = Some(ToolArgsFailureDetail {
                                category: parse_error_category(&corrected_error),
                                detail: corrected_error.to_string(),
                            });
                        }
                    }
                } else {
                    emit_tool_args_fim_correction_failed(
                        &pending.call_id,
                        &pending.name,
                        primary_error_category,
                    );
                }
            }

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
            } else if let Some(correction_failure) = correction_failure {
                match correction_failure.category {
                    "schema_validation" => CapabilityResult::failure(format!(
                        "JSON SCHEMA VALIDATION ERROR in corrected tool arguments: {}\n\
                         Fix: ensure the corrected JSON satisfies the tool schema exactly, \
                         including numeric/string constraints, enums, array item rules, and \
                         required fields. Then retry this tool call.",
                        correction_failure.detail
                    )),
                    _ => CapabilityResult::failure(format!(
                        "JSON PARSE ERROR in corrected tool arguments: {}\n\
                         Fix: ensure the corrected output is exactly one valid JSON object with \
                         no markdown fences or trailing text. Then retry this tool call.",
                        correction_failure.detail
                    )),
                }
            } else {
                CapabilityResult::failure(format!(
                    "JSON PARSE ERROR in tool arguments: {error}\n\
                     Fix: ensure all double quotes inside string values are escaped \
                     as \\\", newlines are \\n, and JSON containers are fully closed. \
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
        if let Some(repaired_combo) = repair_truncated_json_closure(&repaired_quotes)
            && serde_json::from_str::<serde_json::Value>(&repaired_combo).is_ok()
        {
            return Some((repaired_combo, "unescaped quotes + truncated payload"));
        }
    }

    if let Some(repaired_truncated) = repair_truncated_json_closure(raw)
        && serde_json::from_str::<serde_json::Value>(&repaired_truncated).is_ok()
    {
        return Some((repaired_truncated, "truncated payload"));
    }

    None
}

fn tool_args_failure_key(tool_name: &str, cleaned_args: &str) -> (String, String) {
    (tool_name.to_string(), cleaned_args.to_string())
}

fn validate_fim_corrected_args(
    tools: &ToolRegistry,
    tool_name: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    let Some(tool) = tools.get(tool_name) else {
        return Ok(());
    };

    let schema = tool.spec().parameters;
    let compiled = JSONSchema::options()
        .compile(&schema)
        .map_err(|error| format!("invalid tool schema for `{tool_name}`: {error}"))?;

    match compiled.validate(value) {
        Ok(()) => Ok(()),
        Err(errors) => Err(errors
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("; ")),
    }
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
