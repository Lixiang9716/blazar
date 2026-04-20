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

                run_turn_with_retry(
                    &turn_id,
                    &prompt,
                    &*provider,
                    &tools,
                    &event_tx,
                    &cancel_flag,
                );
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
    provider: &dyn LlmProvider,
    tools: &ToolRegistry,
    event_tx: &Sender<AgentEvent>,
    cancel_flag: &Arc<AtomicBool>,
) {
    for attempt in 0..=MAX_TRANSIENT_RETRIES {
        if cancel_flag.load(Ordering::SeqCst) {
            info!("runtime: turn {turn_id} cancelled before attempt {attempt}");
            let _ = event_tx.send(AgentEvent::TurnFailed {
                error: "cancelled".to_string(),
            });
            return;
        }

        let result = run_turn(prompt, provider, tools, event_tx, cancel_flag);

        match result {
            TurnOutcome::Complete => {
                let _ = event_tx.send(AgentEvent::TurnComplete);
                return;
            }
            TurnOutcome::Cancelled => return,
            TurnOutcome::TransientError(err) => {
                if attempt < MAX_TRANSIENT_RETRIES {
                    warn!(
                        "runtime: transient error on turn {turn_id} attempt {attempt}: {err}, retrying"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(500));
                } else {
                    warn!("runtime: turn {turn_id} failed after {attempt} retries: {err}");
                    let _ = event_tx.send(AgentEvent::TurnFailed { error: err });
                }
            }
            TurnOutcome::FatalError(err) => {
                warn!("runtime: turn {turn_id} fatal error: {err}");
                let _ = event_tx.send(AgentEvent::TurnFailed { error: err });
                return;
            }
        }
    }
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
    prompt: &str,
    provider: &dyn LlmProvider,
    tools: &ToolRegistry,
    event_tx: &Sender<AgentEvent>,
    cancel_flag: &Arc<AtomicBool>,
) -> TurnOutcome {
    let mut messages = vec![ProviderMessage::User {
        content: prompt.to_string(),
    }];
    let tool_specs = tools.specs();
    let mut tool_iterations = 0usize;

    loop {
        if cancel_flag.load(Ordering::SeqCst) {
            let _ = event_tx.send(AgentEvent::TurnFailed {
                error: "cancelled".to_string(),
            });
            return TurnOutcome::Cancelled;
        }

        let pass = stream_provider_pass(provider, &messages, &tool_specs, event_tx, cancel_flag);

        match pass.outcome {
            TurnOutcome::Complete => {
                if pass.tool_calls.is_empty() {
                    return TurnOutcome::Complete;
                }

                if !pass.assistant_text.is_empty() {
                    messages.push(ProviderMessage::Assistant {
                        content: pass.assistant_text,
                    });
                }

                for pending in pass.tool_calls {
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

                    let result = match serde_json::from_str::<Value>(&pending.arguments) {
                        Ok(args) => execute_tool_call(tools, &pending.name, args),
                        Err(error) => {
                            ToolResult::failure(format!("invalid tool arguments: {error}"))
                        }
                    };

                    let _ = event_tx.send(AgentEvent::ToolCallCompleted {
                        call_id: pending.call_id.clone(),
                        output: result.output.clone(),
                        is_error: result.is_error,
                    });

                    messages.push(ProviderMessage::ToolCall {
                        id: pending.call_id.clone(),
                        name: pending.name.clone(),
                        arguments: pending.arguments.clone(),
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
            TurnOutcome::TransientError(err) => return TurnOutcome::TransientError(err),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::ToolSpec;

    fn empty_registry() -> ToolRegistry {
        ToolRegistry::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    }

    fn user_messages(prompt: &str) -> Vec<ProviderMessage> {
        vec![ProviderMessage::User {
            content: prompt.to_string(),
        }]
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

        let outcome = run_turn("hi", &provider, &empty_registry(), &event_tx, &cancel);
        assert!(matches!(outcome, TurnOutcome::Complete));

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
                run_turn("test", &provider, &empty_registry(), &event_tx, &cancel2);
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
        let outcome = run_turn(
            "hi",
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
        let outcome = run_turn(
            "hi",
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

        run_turn_with_retry(
            "turn-test",
            "hi",
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

        run_turn_with_retry(
            "turn-test",
            "hi",
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

        run_turn_with_retry(
            "turn-test",
            "hi",
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

        run_turn_with_retry(
            "turn-test",
            "hi",
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
}
