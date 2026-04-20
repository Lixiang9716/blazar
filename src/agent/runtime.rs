use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use log::{debug, info, warn};
use serde_json::Value;

use super::protocol::{AgentCommand, AgentEvent};
use super::tools::ToolRegistry;
use crate::agent::tools::ToolResult;
use crate::agent::tools::bash::BashTool;
use crate::agent::tools::list_dir::ListDirTool;
use crate::agent::tools::read_file::ReadFileTool;
use crate::agent::tools::write_file::WriteFileTool;
use crate::provider::{LlmProvider, ProviderEvent, ProviderMessage};

/// The agent runtime bridges the synchronous TUI render loop and
/// the (potentially blocking) LLM provider.
///
/// It spawns a background thread that:
/// 1. Waits for `AgentCommand`s from the UI.
/// 2. Runs the provider in a scoped sub-thread for real-time streaming.
/// 3. Relays `AgentEvent`s back to the UI via a channel.
///
/// The UI calls `try_recv()` each tick to drain available events.
pub struct AgentRuntime {
    cmd_tx: Sender<AgentCommand>,
    event_rx: Receiver<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

/// Maximum number of transient-error retries per turn.
const MAX_TRANSIENT_RETRIES: u32 = 1;
const MAX_TOOL_ITERATIONS: usize = 10;
const REPEATED_SUCCESS_GUIDANCE: &str = "REPEATED SUCCESS: identical tool call already succeeded in this turn. \
     Stop repeating it and continue with the next step or final answer.";
const JSON_REPAIR_NOTE: &str = "[NOTE] Tool arguments had malformed JSON and were auto-repaired. \
Use escaped double quotes (\\\") inside JSON string values.";

impl AgentRuntime {
    /// Create a new runtime with the given provider.
    pub fn new(provider: Box<dyn LlmProvider>, workspace_root: PathBuf) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&cancel_flag);

        let handle = std::thread::Builder::new()
            .name("blazar-agent".into())
            .spawn(move || {
                let mut tools = ToolRegistry::new(workspace_root.clone());
                tools.register(Box::new(ReadFileTool::new(workspace_root.clone())));
                tools.register(Box::new(WriteFileTool::new(workspace_root.clone())));
                tools.register(Box::new(ListDirTool::new(workspace_root.clone())));
                tools.register(Box::new(BashTool::new(workspace_root)));
                runtime_loop(cmd_rx, event_tx, provider, tools, flag_clone)
            })
            .expect("failed to spawn agent runtime thread");

        Self {
            cmd_tx,
            event_rx,
            cancel_flag,
            handle: Some(handle),
        }
    }

    /// Submit a new turn to the agent.
    ///
    /// Returns `Err` if the runtime channel is closed.
    pub fn submit_turn(&self, prompt: &str) -> Result<(), String> {
        self.cancel_flag.store(false, Ordering::SeqCst);
        self.cmd_tx
            .send(AgentCommand::SubmitTurn {
                prompt: prompt.to_string(),
            })
            .map_err(|_| "agent runtime channel closed".to_string())
    }

    /// Cancel the current turn. The provider sub-thread will stop
    /// relaying events once it observes the flag.
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        let _ = self.cmd_tx.send(AgentCommand::Cancel);
    }

    /// Non-blocking poll for the next event. Returns `None` if no event
    /// is available. Call this in the render-loop tick.
    pub fn try_recv(&self) -> Option<AgentEvent> {
        self.event_rx.try_recv().ok()
    }
}

impl Drop for AgentRuntime {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(AgentCommand::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// The main loop running on the background thread.
fn runtime_loop(
    cmd_rx: Receiver<AgentCommand>,
    event_tx: Sender<AgentEvent>,
    provider: Box<dyn LlmProvider>,
    tools: ToolRegistry,
    cancel_flag: Arc<AtomicBool>,
) {
    let mut turn_counter = 0u64;
    let mut conversation_history: Vec<ProviderMessage> = Vec::new();

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            AgentCommand::SubmitTurn { prompt } => {
                turn_counter += 1;
                let turn_id = format!("turn-{turn_counter}");
                info!(
                    "runtime: SubmitTurn id={turn_id} prompt_len={}",
                    prompt.len()
                );

                cancel_flag.store(false, Ordering::SeqCst);

                if event_tx
                    .send(AgentEvent::TurnStarted {
                        turn_id: turn_id.clone(),
                    })
                    .is_err()
                {
                    break;
                }

                if let Some(updated_history) = run_turn_with_retry(
                    &turn_id,
                    &prompt,
                    &conversation_history,
                    &*provider,
                    &tools,
                    &event_tx,
                    &cancel_flag,
                ) {
                    conversation_history = updated_history;
                }
            }
            AgentCommand::Cancel => {
                debug!("runtime: Cancel received");
                cancel_flag.store(true, Ordering::SeqCst);
            }
            AgentCommand::Shutdown => {
                info!("runtime: Shutdown");
                break;
            }
        }
    }
}

/// Execute a turn with up to `MAX_TRANSIENT_RETRIES` retries on transient errors.
fn run_turn_with_retry(
    turn_id: &str,
    prompt: &str,
    history: &[ProviderMessage],
    provider: &dyn LlmProvider,
    tools: &ToolRegistry,
    event_tx: &Sender<AgentEvent>,
    cancel_flag: &Arc<AtomicBool>,
) -> Option<Vec<ProviderMessage>> {
    for attempt in 0..=MAX_TRANSIENT_RETRIES {
        if cancel_flag.load(Ordering::SeqCst) {
            info!("runtime: turn {turn_id} cancelled before attempt {attempt}");
            let _ = event_tx.send(AgentEvent::TurnFailed {
                error: "cancelled".to_string(),
            });
            return None;
        }

        let mut messages = history.to_vec();
        messages.push(ProviderMessage::User {
            content: prompt.to_string(),
        });
        let result = run_turn(&mut messages, provider, tools, event_tx, cancel_flag);

        match result {
            TurnOutcome::Complete => {
                let _ = event_tx.send(AgentEvent::TurnComplete);
                return Some(messages);
            }
            TurnOutcome::Cancelled => return None,
            TurnOutcome::TransientError(err) => {
                if attempt < MAX_TRANSIENT_RETRIES {
                    warn!(
                        "runtime: transient error on turn {turn_id} attempt {attempt}: {err}, retrying"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(500));
                } else {
                    warn!("runtime: turn {turn_id} failed after {attempt} retries: {err}");
                    let _ = event_tx.send(AgentEvent::TurnFailed { error: err });
                    return None;
                }
            }
            TurnOutcome::FatalError(err) => {
                warn!("runtime: turn {turn_id} fatal error: {err}");
                let _ = event_tx.send(AgentEvent::TurnFailed { error: err });
                return None;
            }
        }
    }

    None
}

enum TurnOutcome {
    Complete,
    Cancelled,
    TransientError(String),
    FatalError(String),
}

struct PendingToolCall {
    call_id: String,
    name: String,
    arguments: String,
}

struct ProviderPass {
    outcome: TurnOutcome,
    assistant_text: String,
    tool_calls: Vec<PendingToolCall>,
}

struct ParsedToolArgs {
    value: Value,
    was_repaired: bool,
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
fn run_turn(
    messages: &mut Vec<ProviderMessage>,
    provider: &dyn LlmProvider,
    tools: &ToolRegistry,
    event_tx: &Sender<AgentEvent>,
    cancel_flag: &Arc<AtomicBool>,
) -> TurnOutcome {
    let tool_specs = tools.specs();
    let mut tool_iterations = 0usize;
    // Track (tool_name, raw_args) → consecutive failure count for duplicate detection.
    let mut consecutive_failures: HashMap<(String, String), usize> = HashMap::new();
    let mut last_success_signature: Option<(String, String)> = None;

    loop {
        if cancel_flag.load(Ordering::SeqCst) {
            let _ = event_tx.send(AgentEvent::TurnFailed {
                error: "cancelled".to_string(),
            });
            return TurnOutcome::Cancelled;
        }

        let pass = stream_provider_pass(provider, messages, &tool_specs, event_tx, cancel_flag);

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

                for pending in pending_calls {
                    if cancel_flag.load(Ordering::SeqCst) {
                        let _ = event_tx.send(AgentEvent::TurnFailed {
                            error: "cancelled".to_string(),
                        });
                        return TurnOutcome::Cancelled;
                    }

                    if tool_iterations >= MAX_TOOL_ITERATIONS {
                        return TurnOutcome::FatalError("tool iteration limit exceeded".into());
                    }

                    let _ = event_tx.send(AgentEvent::ToolCallStarted {
                        call_id: pending.call_id.clone(),
                        tool_name: pending.name.clone(),
                        arguments: pending.arguments.clone(),
                    });

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
                            if last_success_signature.as_ref() == Some(&signature) {
                                ToolResult::failure(REPEATED_SUCCESS_GUIDANCE)
                            } else {
                                let mut result =
                                    execute_tool_call(tools, &pending.name, parsed.value);
                                if parsed.was_repaired {
                                    result.output =
                                        format!("{}\n\n{}", JSON_REPAIR_NOTE, result.output);
                                }
                                if result.is_error {
                                    last_success_signature = None;
                                } else {
                                    last_success_signature = Some(signature);
                                }
                                result
                            }
                        }
                        Err(error) => {
                            last_success_signature = None;
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
                                    "REPEATED ERROR: you sent identical malformed arguments. \
                                     Please change approach: use single quotes for strings \
                                     inside code, or escape double quotes with backslash (\\\")"
                                        .to_string(),
                                )
                            } else {
                                ToolResult::failure(format!(
                                    "invalid tool arguments: JSON parse error. \
                                     Ensure double quotes inside string values are \
                                     escaped with backslash (\\\"): {error}"
                                ))
                            }
                        }
                    };

                    let _ = event_tx.send(AgentEvent::ToolCallCompleted {
                        call_id: pending.call_id.clone(),
                        output: result.output.clone(),
                        is_error: result.is_error,
                    });

                    messages.push(ProviderMessage::ToolResult {
                        tool_call_id: pending.call_id,
                        output: result.output,
                        is_error: result.is_error,
                    });
                    tool_iterations += 1;
                }
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

fn stream_provider_pass(
    provider: &dyn LlmProvider,
    messages: &[ProviderMessage],
    tool_specs: &[crate::agent::tools::ToolSpec],
    event_tx: &Sender<AgentEvent>,
    cancel_flag: &Arc<AtomicBool>,
) -> ProviderPass {
    let (chunk_tx, chunk_rx) = mpsc::channel::<ProviderEvent>();
    let mut pass = ProviderPass {
        outcome: TurnOutcome::Complete,
        assistant_text: String::new(),
        tool_calls: Vec::new(),
    };

    std::thread::scope(|scope| {
        scope.spawn(|| provider.stream_turn(messages, tool_specs, chunk_tx));

        for event in &chunk_rx {
            if cancel_flag.load(Ordering::SeqCst) {
                info!("stream_provider_pass: cancel flag observed, stopping relay");
                let _ = event_tx.send(AgentEvent::TurnFailed {
                    error: "cancelled".to_string(),
                });
                pass.outcome = TurnOutcome::Cancelled;
                return;
            }

            match event {
                ProviderEvent::TextDelta(text) => {
                    pass.assistant_text.push_str(&text);
                    if event_tx.send(AgentEvent::TextDelta { text }).is_err() {
                        break;
                    }
                }
                ProviderEvent::ThinkingDelta(text) => {
                    if event_tx.send(AgentEvent::ThinkingDelta { text }).is_err() {
                        break;
                    }
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

/// Strip `<think>...</think>` reasoning blocks that some models (e.g. Qwen3)
/// may embed in tool call arguments. Falls back to the original string if no
/// tags are found. Also attempts to extract a JSON substring if the result
/// still doesn't start with `{` or `[`.
fn strip_thinking_tags(raw: &str) -> String {
    let mut s = raw.to_string();

    // Remove <think>...</think> blocks (greedy across lines).
    if let Some(start) = s.find("<think>")
        && let Some(end) = s.find("</think>")
    {
        let tag_end = end + "</think>".len();
        s = format!("{}{}", &s[..start], s[tag_end..].trim_start());
    }

    // If the result still doesn't look like JSON, try to find the first `{`.
    let trimmed = s.trim();
    if !trimmed.starts_with('{')
        && !trimmed.starts_with('[')
        && let Some(idx) = trimmed.find('{')
    {
        return trimmed[idx..].to_string();
    }

    s
}

/// Try standard JSON parse first. On failure, attempt targeted repair of
/// unescaped interior quotes (common with Qwen3 models that embed code
/// containing `"` inside JSON string values).
fn parse_or_repair_json(raw: &str) -> Result<ParsedToolArgs, serde_json::Error> {
    let first_try = serde_json::from_str::<Value>(raw);
    if let Ok(value) = &first_try {
        return Ok(ParsedToolArgs {
            value: value.clone(),
            was_repaired: false,
        });
    }

    // Attempt iterative repair — escape the likely-problematic quote
    // near each successive parse-error position.
    if let Some(repaired) = repair_json_arguments(raw)
        && let Ok(val) = serde_json::from_str::<Value>(&repaired)
    {
        warn!(
            "runtime: repaired malformed JSON arguments\n  before: {}\n  after:  {}",
            preview_text(raw, 200),
            preview_text(&repaired, 200),
        );
        return Ok(ParsedToolArgs {
            value: val,
            was_repaired: true,
        });
    }

    // Return the original error for a meaningful message.
    first_try.map(|value| ParsedToolArgs {
        value,
        was_repaired: false,
    })
}

fn canonical_tool_args(value: &Value, fallback: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| fallback.to_string())
}

/// Iterative targeted repair: find the unescaped `"` just before each
/// parse-error column, escape it, and retry.  Stops after `MAX_REPAIRS`
/// successful individual fixes or when no more fixable quotes are found.
fn repair_json_arguments(raw: &str) -> Option<String> {
    const MAX_REPAIRS: usize = 8;
    let mut s = raw.to_string();

    for _ in 0..MAX_REPAIRS {
        let err = match serde_json::from_str::<Value>(&s) {
            Ok(_) => return Some(s),
            Err(e) => e,
        };

        // serde_json reports 1-based column; convert to 0-based byte index.
        let col = err.column().saturating_sub(1);
        if col == 0 {
            return None;
        }

        let bytes = s.as_bytes();
        let search_end = col.min(bytes.len());

        // Walk backward to find the last unescaped `"` before the error.
        let mut found = None;
        let mut i = search_end;
        while i > 0 {
            i -= 1;
            if bytes[i] == b'"' {
                // Count preceding backslashes to determine parity.
                let mut bs = 0usize;
                let mut j = i;
                while j > 0 && bytes[j - 1] == b'\\' {
                    bs += 1;
                    j -= 1;
                }
                if bs.is_multiple_of(2) {
                    // This `"` is NOT escaped — candidate for repair.
                    found = Some(i);
                    break;
                }
            }
        }

        if let Some(pos) = found {
            s.insert(pos, '\\');
        } else {
            return None; // nothing left to fix
        }
    }

    // Final validation after all repairs.
    match serde_json::from_str::<Value>(&s) {
        Ok(_) => Some(s),
        Err(_) => None,
    }
}

/// Safe UTF-8 text preview for logging.
fn preview_text(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &text[..byte_idx],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::{Tool, ToolSpec};
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU32;

    fn empty_registry() -> ToolRegistry {
        ToolRegistry::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    }

    fn user_messages(prompt: &str) -> Vec<ProviderMessage> {
        vec![ProviderMessage::User {
            content: prompt.to_string(),
        }]
    }

    struct CountingTool {
        calls: Arc<AtomicU32>,
    }

    impl Tool for CountingTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "count".into(),
                description: "count executions".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
            }
        }

        fn execute(&self, _args: serde_json::Value) -> crate::agent::tools::ToolResult {
            self.calls.fetch_add(1, Ordering::SeqCst);
            crate::agent::tools::ToolResult::success("counted")
        }
    }

    #[test]
    fn transient_errors_classified_correctly() {
        assert!(is_transient_error("connection timeout"));
        assert!(is_transient_error("HTTP 429 Too Many Requests"));
        assert!(is_transient_error("502 Bad Gateway"));
        assert!(is_transient_error("503 Service Unavailable"));
        assert!(is_transient_error("connection reset by peer"));
        assert!(is_transient_error("rate limit exceeded"));
    }

    #[test]
    fn fatal_errors_classified_correctly() {
        assert!(!is_transient_error("invalid API key"));
        assert!(!is_transient_error("400 Bad Request"));
        assert!(!is_transient_error("model not found"));
        assert!(!is_transient_error("content policy violation"));
        assert!(!is_transient_error(""));
    }

    #[test]
    fn transient_classification_is_case_insensitive() {
        assert!(is_transient_error("CONNECTION TIMEOUT"));
        assert!(is_transient_error("Rate Limit"));
        assert!(is_transient_error("Timeout Error"));
    }

    #[test]
    fn run_turn_completes_with_echo_provider() {
        let provider = crate::provider::echo::EchoProvider::new(0);
        let (event_tx, event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        let mut messages = user_messages("hi");
        let outcome = run_turn(
            &mut messages,
            &provider,
            &empty_registry(),
            &event_tx,
            &cancel,
        );
        assert!(matches!(outcome, TurnOutcome::Complete));
        assert!(messages.iter().any(|message| matches!(
            message,
            ProviderMessage::Assistant { content } if content == "Echo: hi"
        )));

        let text: String = event_rx
            .try_iter()
            .filter_map(|event| match event {
                AgentEvent::TextDelta { text } => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(text, "Echo: hi");
    }

    #[test]
    fn run_turn_stops_on_cancel_flag() {
        use std::sync::Barrier;

        struct SlowProvider {
            barrier: Arc<Barrier>,
        }

        impl LlmProvider for SlowProvider {
            fn stream_turn(
                &self,
                _messages: &[ProviderMessage],
                _tools: &[ToolSpec],
                tx: Sender<ProviderEvent>,
            ) {
                let _ = tx.send(ProviderEvent::TextDelta("chunk1".into()));
                self.barrier.wait();
                for i in 0..100 {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    if tx.send(ProviderEvent::TextDelta(format!("c{i}"))).is_err() {
                        return;
                    }
                }
                let _ = tx.send(ProviderEvent::TurnComplete);
            }
        }

        let barrier = Arc::new(Barrier::new(2));
        let provider = SlowProvider {
            barrier: Arc::clone(&barrier),
        };
        let (event_tx, event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel2 = Arc::clone(&cancel);

        std::thread::scope(|scope| {
            scope.spawn(|| {
                let mut messages = user_messages("test");
                run_turn(
                    &mut messages,
                    &provider,
                    &empty_registry(),
                    &event_tx,
                    &cancel2,
                );
            });

            barrier.wait();
            cancel.store(true, Ordering::SeqCst);
        });

        let events: Vec<_> = event_rx.try_iter().collect();
        let has_cancelled = events
            .iter()
            .any(|event| matches!(event, AgentEvent::TurnFailed { error } if error == "cancelled"));
        assert!(has_cancelled, "should emit TurnFailed with 'cancelled'");
    }

    #[test]
    fn run_turn_returns_transient_on_timeout_error() {
        struct TimeoutProvider;

        impl LlmProvider for TimeoutProvider {
            fn stream_turn(
                &self,
                _messages: &[ProviderMessage],
                _tools: &[ToolSpec],
                tx: Sender<ProviderEvent>,
            ) {
                let _ = tx.send(ProviderEvent::Error("connection timeout".into()));
            }
        }

        let (event_tx, _event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut messages = user_messages("hi");
        let outcome = run_turn(
            &mut messages,
            &TimeoutProvider,
            &empty_registry(),
            &event_tx,
            &cancel,
        );
        assert!(matches!(outcome, TurnOutcome::TransientError(_)));
    }

    #[test]
    fn run_turn_returns_fatal_on_auth_error() {
        struct AuthErrorProvider;

        impl LlmProvider for AuthErrorProvider {
            fn stream_turn(
                &self,
                _messages: &[ProviderMessage],
                _tools: &[ToolSpec],
                tx: Sender<ProviderEvent>,
            ) {
                let _ = tx.send(ProviderEvent::Error("invalid API key".into()));
            }
        }

        let (event_tx, _event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut messages = user_messages("hi");
        let outcome = run_turn(
            &mut messages,
            &AuthErrorProvider,
            &empty_registry(),
            &event_tx,
            &cancel,
        );
        assert!(matches!(outcome, TurnOutcome::FatalError(_)));
    }

    #[test]
    fn retry_recovers_from_transient_error() {
        use std::sync::atomic::AtomicU32;

        struct FailOnceProvider {
            call_count: AtomicU32,
        }

        impl LlmProvider for FailOnceProvider {
            fn stream_turn(
                &self,
                _messages: &[ProviderMessage],
                _tools: &[ToolSpec],
                tx: Sender<ProviderEvent>,
            ) {
                let n = self.call_count.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    let _ = tx.send(ProviderEvent::Error("connection timeout".into()));
                } else {
                    let _ = tx.send(ProviderEvent::TextDelta("ok".into()));
                    let _ = tx.send(ProviderEvent::TurnComplete);
                }
            }
        }

        let provider = FailOnceProvider {
            call_count: AtomicU32::new(0),
        };
        let (event_tx, event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        let _ = run_turn_with_retry(
            "turn-test",
            "hi",
            &[],
            &provider,
            &empty_registry(),
            &event_tx,
            &cancel,
        );

        let events: Vec<_> = event_rx.try_iter().collect();
        let has_complete = events
            .iter()
            .any(|event| matches!(event, AgentEvent::TurnComplete));
        assert!(has_complete, "retry should succeed on second attempt");
        assert_eq!(provider.call_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn retry_gives_up_after_max_attempts() {
        struct AlwaysTimeoutProvider;

        impl LlmProvider for AlwaysTimeoutProvider {
            fn stream_turn(
                &self,
                _messages: &[ProviderMessage],
                _tools: &[ToolSpec],
                tx: Sender<ProviderEvent>,
            ) {
                let _ = tx.send(ProviderEvent::Error("timeout".into()));
            }
        }

        let (event_tx, event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        let _ = run_turn_with_retry(
            "turn-test",
            "hi",
            &[],
            &AlwaysTimeoutProvider,
            &empty_registry(),
            &event_tx,
            &cancel,
        );

        let events: Vec<_> = event_rx.try_iter().collect();
        let has_failed = events
            .iter()
            .any(|event| matches!(event, AgentEvent::TurnFailed { .. }));
        assert!(
            has_failed,
            "should emit TurnFailed after exhausting retries"
        );
    }

    #[test]
    fn fatal_error_skips_retry() {
        use std::sync::atomic::AtomicU32;

        struct FatalProvider {
            call_count: AtomicU32,
        }

        impl LlmProvider for FatalProvider {
            fn stream_turn(
                &self,
                _messages: &[ProviderMessage],
                _tools: &[ToolSpec],
                tx: Sender<ProviderEvent>,
            ) {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                let _ = tx.send(ProviderEvent::Error("invalid API key".into()));
            }
        }

        let provider = FatalProvider {
            call_count: AtomicU32::new(0),
        };
        let (event_tx, event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        let _ = run_turn_with_retry(
            "turn-test",
            "hi",
            &[],
            &provider,
            &empty_registry(),
            &event_tx,
            &cancel,
        );

        assert_eq!(
            provider.call_count.load(Ordering::SeqCst),
            1,
            "fatal error should not retry"
        );

        let events: Vec<_> = event_rx.try_iter().collect();
        let has_failed = events
            .iter()
            .any(|event| matches!(event, AgentEvent::TurnFailed { .. }));
        assert!(has_failed);
    }

    #[test]
    fn cancel_before_retry_attempt_stops_immediately() {
        struct TimeoutProvider;

        impl LlmProvider for TimeoutProvider {
            fn stream_turn(
                &self,
                _messages: &[ProviderMessage],
                _tools: &[ToolSpec],
                tx: Sender<ProviderEvent>,
            ) {
                let _ = tx.send(ProviderEvent::Error("timeout".into()));
            }
        }

        let (event_tx, event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(true));

        let _ = run_turn_with_retry(
            "turn-test",
            "hi",
            &[],
            &TimeoutProvider,
            &empty_registry(),
            &event_tx,
            &cancel,
        );

        let events: Vec<_> = event_rx.try_iter().collect();
        let has_cancelled = events
            .iter()
            .any(|event| matches!(event, AgentEvent::TurnFailed { error } if error == "cancelled"));
        assert!(
            has_cancelled,
            "pre-cancelled flag should abort before first attempt"
        );
    }

    #[test]
    fn retry_does_not_rerun_tools_after_transient_error() {
        struct ToolThenTransientProvider {
            stage: AtomicU32,
        }

        impl LlmProvider for ToolThenTransientProvider {
            fn stream_turn(
                &self,
                messages: &[ProviderMessage],
                _tools: &[ToolSpec],
                tx: Sender<ProviderEvent>,
            ) {
                let has_tool_result = messages
                    .iter()
                    .any(|message| matches!(message, ProviderMessage::ToolResult { .. }));
                let stage = self.stage.load(Ordering::SeqCst);

                match (stage, has_tool_result) {
                    (0, false) | (1, false) => {
                        let _ = tx.send(ProviderEvent::ToolCall {
                            call_id: "call-1".into(),
                            name: "count".into(),
                            arguments: "{}".into(),
                        });
                        let _ = tx.send(ProviderEvent::TurnComplete);
                    }
                    (0, true) => {
                        self.stage.store(1, Ordering::SeqCst);
                        let _ = tx.send(ProviderEvent::Error("connection timeout".into()));
                    }
                    (1, true) => {
                        let _ = tx.send(ProviderEvent::TextDelta("done".into()));
                        let _ = tx.send(ProviderEvent::TurnComplete);
                    }
                    _ => unreachable!("unexpected provider stage"),
                }
            }
        }

        let counter = Arc::new(AtomicU32::new(0));
        let mut registry = empty_registry();
        registry.register(Box::new(CountingTool {
            calls: Arc::clone(&counter),
        }));

        let provider = ToolThenTransientProvider {
            stage: AtomicU32::new(0),
        };
        let (event_tx, event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        let _ = run_turn_with_retry(
            "turn-test",
            "count once",
            &[],
            &provider,
            &registry,
            &event_tx,
            &cancel,
        );

        let events: Vec<_> = event_rx.try_iter().collect();
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "transient retries must not rerun tool side effects"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::TurnFailed { .. })),
            "turn should fail instead of retrying after tool execution"
        );
    }

    #[test]
    fn run_turn_enforces_tool_iteration_limit() {
        struct InfiniteToolProvider;

        impl LlmProvider for InfiniteToolProvider {
            fn stream_turn(
                &self,
                _messages: &[ProviderMessage],
                _tools: &[ToolSpec],
                tx: Sender<ProviderEvent>,
            ) {
                let _ = tx.send(ProviderEvent::ToolCall {
                    call_id: "call-loop".into(),
                    name: "count".into(),
                    arguments: "{}".into(),
                });
                let _ = tx.send(ProviderEvent::TurnComplete);
            }
        }

        let counter = Arc::new(AtomicU32::new(0));
        let mut registry = empty_registry();
        registry.register(Box::new(CountingTool {
            calls: Arc::clone(&counter),
        }));

        let (event_tx, _event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        let mut messages = user_messages("count forever");
        let outcome = run_turn(
            &mut messages,
            &InfiniteToolProvider,
            &registry,
            &event_tx,
            &cancel,
        );

        assert!(matches!(
            outcome,
            TurnOutcome::FatalError(ref err) if err == "tool iteration limit exceeded"
        ));
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "duplicate success guard should prevent rerunning identical side effects"
        );
    }

    #[test]
    fn run_turn_blocks_repeated_identical_successful_tool_calls() {
        struct DuplicateSuccessProvider;

        impl LlmProvider for DuplicateSuccessProvider {
            fn stream_turn(
                &self,
                messages: &[ProviderMessage],
                _tools: &[ToolSpec],
                tx: Sender<ProviderEvent>,
            ) {
                let last_tool_result = messages.iter().rev().find_map(|message| match message {
                    ProviderMessage::ToolResult {
                        output, is_error, ..
                    } => Some((output.as_str(), *is_error)),
                    _ => None,
                });

                match last_tool_result {
                    None => {
                        let _ = tx.send(ProviderEvent::ToolCall {
                            call_id: "call-1".into(),
                            name: "count".into(),
                            arguments: "{}".into(),
                        });
                        let _ = tx.send(ProviderEvent::TurnComplete);
                    }
                    Some((output, true)) if output.contains("REPEATED SUCCESS") => {
                        let _ = tx.send(ProviderEvent::TextDelta("done".into()));
                        let _ = tx.send(ProviderEvent::TurnComplete);
                    }
                    Some(_) => {
                        let _ = tx.send(ProviderEvent::ToolCall {
                            call_id: "call-2".into(),
                            name: "count".into(),
                            arguments: "{}".into(),
                        });
                        let _ = tx.send(ProviderEvent::TurnComplete);
                    }
                }
            }
        }

        let counter = Arc::new(AtomicU32::new(0));
        let mut registry = empty_registry();
        registry.register(Box::new(CountingTool {
            calls: Arc::clone(&counter),
        }));

        let (event_tx, _event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut messages = user_messages("count once");

        let outcome = run_turn(
            &mut messages,
            &DuplicateSuccessProvider,
            &registry,
            &event_tx,
            &cancel,
        );

        assert!(matches!(outcome, TurnOutcome::Complete));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert!(messages.iter().any(|message| {
            matches!(message, ProviderMessage::ToolResult { output, is_error, .. }
                if *is_error && output.contains("REPEATED SUCCESS"))
        }));
    }

    #[test]
    fn run_turn_surfaces_json_repair_note_in_tool_result() {
        struct RepairedArgsProvider;

        impl LlmProvider for RepairedArgsProvider {
            fn stream_turn(
                &self,
                messages: &[ProviderMessage],
                _tools: &[ToolSpec],
                tx: Sender<ProviderEvent>,
            ) {
                let has_tool_result = messages
                    .iter()
                    .any(|message| matches!(message, ProviderMessage::ToolResult { .. }));
                if has_tool_result {
                    let _ = tx.send(ProviderEvent::TextDelta("done".into()));
                    let _ = tx.send(ProviderEvent::TurnComplete);
                } else {
                    let _ = tx.send(ProviderEvent::ToolCall {
                        call_id: "call-1".into(),
                        name: "count".into(),
                        arguments: r#"{"content":"print("hello world!\")"}"#.into(),
                    });
                    let _ = tx.send(ProviderEvent::TurnComplete);
                }
            }
        }

        let counter = Arc::new(AtomicU32::new(0));
        let mut registry = empty_registry();
        registry.register(Box::new(CountingTool {
            calls: Arc::clone(&counter),
        }));

        let (event_tx, _event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut messages = user_messages("run repaired args");

        let outcome = run_turn(
            &mut messages,
            &RepairedArgsProvider,
            &registry,
            &event_tx,
            &cancel,
        );
        assert!(matches!(outcome, TurnOutcome::Complete));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert!(messages.iter().any(|message| {
            matches!(message, ProviderMessage::ToolResult { output, is_error, .. }
                if !*is_error && output.contains(JSON_REPAIR_NOTE))
        }));
    }

    #[test]
    fn provider_that_sends_no_terminal_event_gets_auto_complete() {
        struct NoTerminalProvider;

        impl LlmProvider for NoTerminalProvider {
            fn stream_turn(
                &self,
                _messages: &[ProviderMessage],
                _tools: &[ToolSpec],
                tx: Sender<ProviderEvent>,
            ) {
                let _ = tx.send(ProviderEvent::TextDelta("partial".into()));
            }
        }

        let (event_tx, event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        let pass = stream_provider_pass(
            &NoTerminalProvider,
            &user_messages("hi"),
            &[],
            &event_tx,
            &cancel,
        );
        assert!(matches!(pass.outcome, TurnOutcome::Complete));
        assert_eq!(pass.assistant_text, "partial");

        let events: Vec<_> = event_rx.try_iter().collect();
        let has_text = events
            .iter()
            .any(|event| matches!(event, AgentEvent::TextDelta { text } if text == "partial"));
        assert!(has_text, "should relay text even without terminal event");
    }

    #[test]
    fn strip_thinking_tags_removes_think_block() {
        let raw = "<think>\nreasoning here\n</think>\n{\"path\": \"hello.py\"}";
        assert_eq!(strip_thinking_tags(raw), "{\"path\": \"hello.py\"}");
    }

    #[test]
    fn strip_thinking_tags_preserves_clean_json() {
        let raw = "{\"command\": \"ls -la\"}";
        assert_eq!(strip_thinking_tags(raw), raw);
    }

    #[test]
    fn strip_thinking_tags_extracts_json_after_garbage() {
        let raw = "some text before {\"key\": \"val\"}";
        assert_eq!(strip_thinking_tags(raw), "{\"key\": \"val\"}");
    }

    #[test]
    fn strip_thinking_tags_handles_empty_think_block() {
        let raw = "<think></think>{\"a\": 1}";
        assert_eq!(strip_thinking_tags(raw), "{\"a\": 1}");
    }

    #[test]
    fn preview_text_truncates_at_char_boundary() {
        let text = "你好世界hello";
        assert_eq!(preview_text(text, 2), "你好");
        assert_eq!(preview_text(text, 100), text);
    }

    // ---- repair_json_arguments tests ----

    #[test]
    fn repair_fixes_unescaped_quotes_in_print() {
        // The exact pattern Qwen3-8B produces.
        let raw = r#"{"path": "hello.py", "content": "print("hello world!\")"}"#;
        let repaired = repair_json_arguments(raw).expect("should repair");
        let val: Value = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
        assert_eq!(val["content"], "print(\"hello world!\")");
    }

    #[test]
    fn repair_returns_none_for_total_garbage() {
        assert!(repair_json_arguments("not json at all").is_none());
    }

    #[test]
    fn repair_returns_valid_json_unchanged() {
        let raw = r#"{"path": "a.py", "content": "x = 1"}"#;
        let repaired = repair_json_arguments(raw).expect("already valid");
        assert_eq!(repaired, raw);
    }

    #[test]
    fn repair_fixes_multiple_unescaped_quotes() {
        // Code with two unescaped quote pairs.
        let raw = r#"{"content": "say("hello") + say("world")"}"#;
        let repaired = repair_json_arguments(raw).expect("should repair");
        let val: Value = serde_json::from_str(&repaired).expect("valid JSON");
        assert!(val["content"].as_str().unwrap().contains("hello"));
        assert!(val["content"].as_str().unwrap().contains("world"));
    }

    #[test]
    fn parse_or_repair_succeeds_on_valid_json() {
        let raw = r#"{"command": "ls"}"#;
        let parsed = parse_or_repair_json(raw).expect("should parse");
        assert!(!parsed.was_repaired);
        assert_eq!(parsed.value["command"], "ls");
    }

    #[test]
    fn parse_or_repair_recovers_from_unescaped_quotes() {
        let raw = r#"{"path": "hello.py", "content": "print("hello world!\")"}"#;
        let parsed = parse_or_repair_json(raw).expect("should recover via repair");
        assert!(parsed.was_repaired);
        assert_eq!(parsed.value["path"], "hello.py");
    }

    #[test]
    fn parse_or_repair_returns_error_for_garbage() {
        let result = parse_or_repair_json("total garbage");
        assert!(result.is_err());
    }
}
