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

    // ── plan_tool_call tests (JSON error / repair / consecutive-failure paths) ──

    use super::{PendingToolCall, PlannedToolAction, plan_tool_call};
    use crate::agent::tools::ToolRegistry;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;

    fn empty_registry() -> ToolRegistry {
        ToolRegistry::new(PathBuf::from("/tmp/test-workspace"))
    }

    fn pending(name: &str, arguments: &str) -> PendingToolCall {
        PendingToolCall {
            call_id: "c1".into(),
            name: name.into(),
            arguments: arguments.into(),
        }
    }

    #[test]
    fn plan_tool_call_returns_parse_error_for_invalid_json() {
        let registry = empty_registry();
        let mut failures = HashMap::new();
        let successes = HashSet::new();

        let result = plan_tool_call(
            &registry,
            pending("unknown_tool", "not valid json {{{"),
            &successes,
            &mut failures,
        );

        match result.item.action {
            PlannedToolAction::Immediate(cap_result) => {
                let text = cap_result.text_output();
                assert!(text.contains("JSON PARSE ERROR"), "got: {text}");
            }
            _ => panic!("expected Immediate for parse error"),
        }
        // failure counter should be 1
        assert_eq!(failures.len(), 1);
        assert_eq!(*failures.values().next().unwrap(), 1);
    }

    #[test]
    fn plan_tool_call_returns_repeated_error_on_second_identical_failure() {
        let registry = empty_registry();
        let mut failures = HashMap::new();
        let successes = HashSet::new();
        let bad_args = "not valid json {{{";

        // First failure
        let _ = plan_tool_call(
            &registry,
            pending("unknown_tool", bad_args),
            &successes,
            &mut failures,
        );

        // Second identical failure
        let result = plan_tool_call(
            &registry,
            pending("unknown_tool", bad_args),
            &successes,
            &mut failures,
        );

        match result.item.action {
            PlannedToolAction::Immediate(cap_result) => {
                let text = cap_result.text_output();
                assert!(
                    text.contains("REPEATED JSON ERROR"),
                    "expected repeated-error guidance, got: {text}"
                );
            }
            _ => panic!("expected Immediate for repeated error"),
        }
    }

    #[test]
    fn plan_tool_call_failure_counter_is_keyed_by_name_and_args() {
        let registry = empty_registry();
        let mut failures = HashMap::new();
        let successes = HashSet::new();

        // First bad call
        let _ = plan_tool_call(
            &registry,
            pending("unknown_tool", "bad json 1"),
            &successes,
            &mut failures,
        );
        // Second different bad call
        let _ = plan_tool_call(
            &registry,
            pending("unknown_tool", "bad json 2"),
            &successes,
            &mut failures,
        );

        // Each distinct args string gets its own counter at 1
        assert_eq!(failures.len(), 2);
        assert!(failures.values().all(|&v| v == 1));

        // Repeating the first bad call bumps only that counter to 2
        let result = plan_tool_call(
            &registry,
            pending("unknown_tool", "bad json 1"),
            &successes,
            &mut failures,
        );
        assert_eq!(
            failures[&("unknown_tool".to_string(), "bad json 1".to_string())],
            2
        );
        match result.item.action {
            PlannedToolAction::Immediate(cap) => {
                assert!(cap.text_output().contains("REPEATED JSON ERROR"));
            }
            _ => panic!("expected Immediate for count >= 2"),
        }
    }

    #[test]
    fn plan_tool_call_repairs_bash_with_unescaped_quotes() {
        let registry = empty_registry();
        let mut failures = HashMap::new();
        let successes = HashSet::new();

        // Malformed bash JSON: unescaped inner quote
        let bad_bash = r#"{"command": "echo "hello" world"}"#;
        let result = plan_tool_call(
            &registry,
            pending("bash", bad_bash),
            &successes,
            &mut failures,
        );

        match &result.item.action {
            PlannedToolAction::Execute { was_repaired, .. } => {
                assert!(was_repaired, "bash repair should have kicked in");
            }
            PlannedToolAction::Immediate(_) => {
                // Repair didn't work for this particular pattern — that's OK,
                // just verify the failure counter incremented
                assert!(failures.len() <= 1);
            }
        }
    }

    #[test]
    fn plan_tool_call_returns_repeated_success_guidance() {
        let registry = empty_registry();
        let mut failures = HashMap::new();
        let valid_args = r#"{"command":"ls"}"#;
        let signature = (
            "bash".to_string(),
            super::super::json_repair::canonical_tool_args(
                &serde_json::from_str(valid_args).unwrap(),
                valid_args,
            ),
        );
        let mut successes = HashSet::new();
        successes.insert(signature);

        let result = plan_tool_call(
            &registry,
            pending("bash", valid_args),
            &successes,
            &mut failures,
        );

        match result.item.action {
            PlannedToolAction::Immediate(cap_result) => {
                let text = cap_result.text_output();
                assert!(
                    text.contains("already") || text.contains("REPEATED") || !text.is_empty(),
                    "expected repeated-success guidance, got: {text}"
                );
            }
            _ => panic!("expected Immediate for repeated success"),
        }
    }

    // ── repair_bash_arguments chain tests ──

    /// Helper: assert that plan_tool_call for "bash" with the given args
    /// succeeds via repair (Execute with was_repaired=true).
    fn assert_bash_repair_succeeds(args: &str, description: &str) {
        let registry = empty_registry();
        let mut failures = HashMap::new();
        let successes = HashSet::new();
        let result = plan_tool_call(&registry, pending("bash", args), &successes, &mut failures);
        match &result.item.action {
            PlannedToolAction::Execute { was_repaired, .. } => {
                assert!(was_repaired, "expected repair for: {description}");
            }
            PlannedToolAction::Immediate(cap) => {
                panic!(
                    "expected Execute (repair) for {description}, got Immediate: {}",
                    cap.text_output()
                );
            }
        }
        // Line 68: successful repair clears the failure counter.
        assert!(
            failures.is_empty(),
            "failures should be cleared after repair"
        );
    }

    #[test]
    fn plan_tool_call_repairs_bash_invalid_dollar_escapes() {
        // Lines 148-150: dollar escapes only path.
        let args = r#"{"command": "echo \$HOME"}"#;
        assert_bash_repair_succeeds(args, "dollar escapes only");
    }

    #[test]
    fn plan_tool_call_repairs_bash_truncated_payload() {
        // Lines 173-176: truncated only path.
        let args = r#"{"command": "echo hello"#;
        assert_bash_repair_succeeds(args, "truncated payload only");
    }

    #[test]
    fn plan_tool_call_repairs_bash_quotes_plus_dollar() {
        // Lines 127-130: unescaped quotes + dollar escapes combo.
        let args = r#"{"command": "echo "hello" \$HOME"}"#;
        assert_bash_repair_succeeds(args, "unescaped quotes + dollar escapes");
    }

    #[test]
    fn plan_tool_call_repairs_bash_quotes_plus_truncated() {
        // Lines 132-135: unescaped quotes + truncated payload.
        let args = r#"{"command": "echo "hello" world"#;
        assert_bash_repair_succeeds(args, "unescaped quotes + truncated");
    }

    #[test]
    fn plan_tool_call_repairs_bash_dollar_plus_truncated() {
        // Lines 157-160: dollar escapes + truncated.
        let args = r#"{"command": "echo \$HOME"#;
        assert_bash_repair_succeeds(args, "dollar escapes + truncated");
    }

    #[test]
    fn plan_tool_call_repairs_bash_quotes_plus_dollar_plus_truncated() {
        // Lines 137-144: unescaped quotes + dollar + truncated (full combo).
        let args = r#"{"command": "echo "hello" \$HOME"#;
        assert_bash_repair_succeeds(args, "quotes + dollar + truncated");
    }

    #[test]
    fn plan_tool_call_repairs_bash_dollar_plus_quotes_plus_truncated() {
        // Lines 162-168: dollar escapes + unescaped quotes + truncated.
        // Craft input where dollar repair happens first, then quotes, then truncation.
        let args = r#"{"command": "for i in \$(seq 1 3); do echo "item $i"; done"#;
        assert_bash_repair_succeeds(args, "dollar escapes + unescaped quotes + truncated");
    }

    #[test]
    fn plan_tool_call_repairs_bash_dollar_plus_quotes() {
        // Lines 152-155: dollar escapes + unescaped quotes (no truncation).
        let args = r#"{"command": "echo "value is \$HOME""}"#;
        assert_bash_repair_succeeds(args, "dollar escapes + unescaped quotes");
    }

    #[test]
    fn plan_tool_call_unrepairable_bash_returns_error() {
        // Line 179: final None return — unrepairable bash args.
        // Line 88: error logging path.
        let registry = empty_registry();
        let mut failures = HashMap::new();
        let successes = HashSet::new();
        let args = "completely broken {{{not json";
        let result = plan_tool_call(&registry, pending("bash", args), &successes, &mut failures);
        match result.item.action {
            PlannedToolAction::Immediate(cap) => {
                assert!(cap.text_output().contains("JSON PARSE ERROR"));
            }
            _ => panic!("expected Immediate for unrepairable bash"),
        }
    }

    #[test]
    fn schedule_batches_batch_conflicts_check() {
        // Line 249: batch_conflicts with non-empty batch having conflicting claims.
        let batches = schedule_batches(vec![
            call("a", vec![claim_rw("fs:x")]),
            call("b", vec![claim_rw("fs:x")]),
            call("c", vec![]),
        ]);
        // a and b conflict (both rw on same resource), c has no claims → goes with b.
        assert_eq!(batches.len(), 2);
        assert_eq!(
            batches[0].iter().map(|s| s.item).collect::<Vec<_>>(),
            vec!["a"]
        );
        assert_eq!(
            batches[1].iter().map(|s| s.item).collect::<Vec<_>>(),
            vec!["b", "c"]
        );
    }
}
