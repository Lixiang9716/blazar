use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use log::{debug, info, warn};

use crate::agent::capability::{CapabilityContentPart, CapabilityResult};
use crate::agent::protocol::AssistantContractDelta;
use crate::agent::tools::ToolRegistry;
use crate::provider::{LlmProvider, ProviderEvent, ProviderMessage};

pub(crate) use super::events::{ChannelObserver, SilentObserver, TurnObserver};
use super::executor::execute_batch;
use super::scheduler::{PendingToolCall, plan_tool_call, schedule_batches};
use super::{MAX_TOOL_ITERATIONS, REPEATED_TIMEOUT_GUIDANCE, RuntimeErrorKind, TIMEOUT_NOTE};

const MAX_RESPONSE_CONTRACT_RETRIES: usize = 2;

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
    let mut response_contract_retries = 0usize;
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
                    if response_contract_enforced(model)
                        && let Err(violations) = validate_response_contract(&pass.assistant_text)
                    {
                        if response_contract_retries >= MAX_RESPONSE_CONTRACT_RETRIES {
                            return TurnOutcome::FatalError {
                                kind: RuntimeErrorKind::ToolExecution,
                                error: format!(
                                    "response contract validation failed after {MAX_RESPONSE_CONTRACT_RETRIES} retries: {}",
                                    violations.join(", ")
                                ),
                            };
                        }

                        response_contract_retries += 1;
                        messages.push(ProviderMessage::User {
                            content: format_response_contract_retry_prompt(&violations),
                        });
                        continue;
                    }

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

pub(crate) fn response_contract_enforced(model: &str) -> bool {
    !model.eq_ignore_ascii_case("echo")
}

pub(crate) fn response_contract_schema_prompt() -> &'static str {
    "Return your final response using EXACTLY this XML schema and no text outside tags:\n\
     <assistant_response>\n\
       <intent>plan|clarify|execute|report</intent>\n\
       <summary>...</summary>\n\
       <tool_summary>...</tool_summary>\n\
       <nextstep>...</nextstep>\n\
       <needs_user_input>true|false</needs_user_input>\n\
       <question>...</question>\n\
       <status>ok|blocked|failed</status>\n\
       <error>...</error>\n\
     </assistant_response>\n\
     Rules: summary/nextstep must be non-empty; if needs_user_input=true then question must be non-empty; if status=failed then error must be non-empty."
}

fn format_response_contract_retry_prompt(violations: &[String]) -> String {
    let mut listed = String::new();
    for violation in violations {
        listed.push_str("- ");
        listed.push_str(violation);
        listed.push('\n');
    }

    format!(
        "Your previous response violated the required XML response contract.\n\
         Violations:\n\
         {listed}\n\
         Re-send a full response.\n\
         {}",
        response_contract_schema_prompt()
    )
}

fn validate_response_contract(payload: &str) -> Result<(), Vec<String>> {
    let trimmed = payload.trim();
    let mut violations = Vec::new();
    if trimmed.is_empty() {
        violations.push("empty_response".to_owned());
        return Err(violations);
    }

    let Some(inner) = trimmed
        .strip_prefix("<assistant_response>")
        .and_then(|rest| rest.strip_suffix("</assistant_response>"))
    else {
        return Err(vec!["missing_assistant_response_root".to_owned()]);
    };

    let mut rest = inner;
    let intent = consume_tag(&mut rest, "intent", &mut violations);
    let summary = consume_tag(&mut rest, "summary", &mut violations);
    let _tool_summary = consume_tag(&mut rest, "tool_summary", &mut violations);
    let nextstep = consume_tag(&mut rest, "nextstep", &mut violations);
    let needs_user_input = consume_tag(&mut rest, "needs_user_input", &mut violations);
    let question = consume_tag(&mut rest, "question", &mut violations);
    let status = consume_tag(&mut rest, "status", &mut violations);
    let error = consume_tag(&mut rest, "error", &mut violations);

    if !rest.trim().is_empty() {
        violations.push("unexpected_content_after_error".to_owned());
    }

    let intent = intent.trim();
    if !matches!(intent, "plan" | "clarify" | "execute" | "report") {
        violations.push("invalid_intent".to_owned());
    }

    if summary.trim().is_empty() {
        violations.push("empty_summary".to_owned());
    }
    if nextstep.trim().is_empty() {
        violations.push("empty_nextstep".to_owned());
    }

    let needs_user_input = match needs_user_input.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => {
            violations.push("invalid_needs_user_input".to_owned());
            None
        }
    };

    let status = match status.trim() {
        "ok" => Some("ok"),
        "blocked" => Some("blocked"),
        "failed" => Some("failed"),
        _ => {
            violations.push("invalid_status".to_owned());
            None
        }
    };

    if matches!(needs_user_input, Some(true)) && question.trim().is_empty() {
        violations.push("question_required_when_needs_user_input_true".to_owned());
    }

    if matches!(status, Some("failed")) && error.trim().is_empty() {
        violations.push("error_required_when_status_failed".to_owned());
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn consume_tag(rest: &mut &str, tag: &str, violations: &mut Vec<String>) -> String {
    let working = rest.trim_start();
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");

    if !working.starts_with(&open) {
        violations.push(format!("missing_or_out_of_order_{tag}"));
        return String::new();
    }

    let after_open = &working[open.len()..];
    let Some(close_index) = after_open.find(&close) else {
        violations.push(format!("missing_closing_{tag}"));
        *rest = "";
        return String::new();
    };

    let value = after_open[..close_index].to_owned();
    *rest = &after_open[close_index + close.len()..];
    value
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
    let contract_side_channel_enabled = response_contract_enforced(model);
    let mut pass = ProviderPass {
        outcome: TurnOutcome::Complete,
        assistant_text: String::new(),
        tool_calls: Vec::new(),
    };
    let mut contract_stream_started = false;
    let mut contract_stream_raw = String::new();
    let mut contract_probe_tail = String::new();
    let mut last_contract_delta: Option<AssistantContractDelta> = None;

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
                    if !contract_side_channel_enabled {
                        observer.on_text_delta(&text);
                        continue;
                    }

                    let mut forward_text_delta = true;
                    if contract_stream_started {
                        contract_stream_raw.push_str(&text);
                        forward_text_delta = false;
                    } else {
                        let mut probe = contract_probe_tail.clone();
                        probe.push_str(&text);
                        if let Some(open_start) = find_contract_open_marker_start(&probe) {
                            contract_stream_started = true;
                            contract_stream_raw.clear();
                            contract_stream_raw.push_str(&probe[open_start..]);
                            contract_probe_tail.clear();
                            forward_text_delta = false;
                        } else {
                            contract_probe_tail = contract_open_probe_tail(&probe);
                            if !contract_probe_tail.is_empty() {
                                // Hold potential split opening-tag fragments until we can disambiguate
                                // on subsequent chunks to avoid leaking protocol markup to UI.
                                forward_text_delta = false;
                            }
                        }
                    }

                    if contract_stream_started
                        && let Some(delta) = parse_assistant_contract_delta(&contract_stream_raw)
                        && last_contract_delta.as_ref() != Some(&delta)
                    {
                        observer.on_assistant_contract_delta(delta.clone());
                        last_contract_delta = Some(delta);
                    }

                    if forward_text_delta {
                        observer.on_text_delta(&text);
                    }
                }
                ProviderEvent::ThinkingDelta(text) => {
                    observer.on_thinking_delta(&text);
                }
                ProviderEvent::Usage(usage) => {
                    observer.on_usage(usage);
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
                    if contract_side_channel_enabled
                        && contract_stream_started
                        && let Some(delta) = parse_assistant_contract_delta(&contract_stream_raw)
                        && last_contract_delta.as_ref() != Some(&delta)
                    {
                        observer.on_assistant_contract_delta(delta.clone());
                        last_contract_delta = Some(delta);
                    }
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

fn find_contract_open_marker_start(text: &str) -> Option<usize> {
    text.rfind("<assistant_response>")
        .or_else(|| text.rfind("<assistant_response"))
}

fn contract_open_probe_tail(text: &str) -> String {
    const MARKER: &str = "<assistant_response";
    let max_len = MARKER.len().saturating_sub(1);
    for (start, _) in text.char_indices().rev() {
        let len = text.len() - start;
        if len > max_len {
            break;
        }
        let suffix = &text[start..];
        if MARKER.starts_with(suffix) {
            return suffix.to_owned();
        }
    }
    String::new()
}

fn parse_assistant_contract_delta(payload: &str) -> Option<AssistantContractDelta> {
    const OPEN: &str = "<assistant_response>";
    const CLOSE: &str = "</assistant_response>";
    let start = payload.rfind(OPEN)?;
    let candidate = &payload[start..];
    let (inner, complete) = match candidate.find(CLOSE) {
        Some(close_index) => (&candidate[OPEN.len()..close_index], true),
        None => (&candidate[OPEN.len()..], false),
    };

    let intent = extract_contract_tag_value(inner, "intent").and_then(normalized_non_empty);
    let summary = extract_contract_tag_value(inner, "summary").and_then(normalized_non_empty);
    let tool_summary =
        extract_contract_tag_value(inner, "tool_summary").and_then(normalized_non_empty);
    let nextstep = extract_contract_tag_value(inner, "nextstep").and_then(normalized_non_empty);
    let question = extract_contract_tag_value(inner, "question").and_then(normalized_non_empty);
    let status = extract_contract_tag_value(inner, "status").and_then(normalized_non_empty);
    let error = extract_contract_tag_value(inner, "error").and_then(normalized_non_empty);
    let needs_user_input =
        extract_contract_tag_value(inner, "needs_user_input").and_then(parse_bool_field);

    if intent.is_none()
        && summary.is_none()
        && tool_summary.is_none()
        && nextstep.is_none()
        && needs_user_input.is_none()
        && question.is_none()
        && status.is_none()
        && error.is_none()
    {
        return None;
    }

    Some(AssistantContractDelta {
        intent,
        summary,
        tool_summary,
        nextstep,
        needs_user_input,
        question,
        status,
        error,
        complete,
    })
}

fn extract_contract_tag_value(inner: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = inner.find(&open)?;
    let after_open = &inner[start + open.len()..];
    if let Some(end) = after_open.find(&close) {
        return Some(after_open[..end].to_owned());
    }
    // Partial streaming value: keep current plain-text fragment before the next tag starts.
    let partial_end = after_open.find('<').unwrap_or(after_open.len());
    let partial = &after_open[..partial_end];
    (!partial.is_empty()).then_some(partial.to_owned())
}

fn normalized_non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed.to_owned())
}

fn parse_bool_field(value: String) -> Option<bool> {
    match value.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}
