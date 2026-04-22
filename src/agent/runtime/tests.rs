use super::*;
use crate::agent::tools::{ResourceAccess, ResourceClaim, Tool, ToolSpec};
use serde_json::json;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU32;
use std::time::Duration;

fn empty_registry() -> ToolRegistry {
    ToolRegistry::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

#[test]
fn new_with_spawner_surfaces_thread_spawn_errors() {
    let result = AgentRuntime::new_with_spawner(
        Box::new(crate::provider::echo::EchoProvider::new(0)),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        "echo".to_owned(),
        |_| Err(std::io::Error::other("spawn failed")),
    );
    let error = match result {
        Ok(_) => panic!("runtime creation should fail"),
        Err(error) => error,
    };

    assert!(matches!(error, AgentRuntimeError::ThreadSpawn(_)));
    assert!(
        error
            .to_string()
            .contains("failed to spawn agent runtime thread")
    );
    assert_eq!(
        error
            .source()
            .expect("thread spawn error should expose source")
            .to_string(),
        "spawn failed"
    );
}

#[test]
fn runtime_loop_handles_cancel_and_shutdown_commands() {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (event_tx, _event_rx) = mpsc::channel();
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_for_thread = Arc::clone(&cancel_flag);
    let tools = empty_registry();

    let handle = std::thread::spawn(move || {
        runtime_loop(
            cmd_rx,
            event_tx,
            Arc::new(crate::provider::echo::EchoProvider::new(0)),
            "echo".to_owned(),
            tools,
            cancel_for_thread,
        );
    });

    cmd_tx.send(AgentCommand::Cancel).expect("send cancel");
    cmd_tx.send(AgentCommand::Shutdown).expect("send shutdown");
    handle.join().expect("runtime loop should stop");

    assert!(cancel_flag.load(Ordering::SeqCst));
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
    let outcome = execute_turn(
        &mut messages,
        &provider,
        "echo",
        &empty_registry(),
        &ChannelObserver { tx: &event_tx },
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
            _model: &str,
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
            execute_turn(
                &mut messages,
                &provider,
                "echo",
                &empty_registry(),
                &ChannelObserver { tx: &event_tx },
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
            _model: &str,
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
    let outcome = execute_turn(
        &mut messages,
        &TimeoutProvider,
        "echo",
        &empty_registry(),
        &ChannelObserver { tx: &event_tx },
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
            _model: &str,
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
    let outcome = execute_turn(
        &mut messages,
        &AuthErrorProvider,
        "echo",
        &empty_registry(),
        &ChannelObserver { tx: &event_tx },
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
            _model: &str,
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
        "echo",
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
            _model: &str,
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
        "echo",
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
            _model: &str,
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
        "echo",
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
            _model: &str,
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
        "echo",
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
            _model: &str,
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
        "echo",
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
            _model: &str,
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
    let outcome = execute_turn(
        &mut messages,
        &InfiniteToolProvider,
        "echo",
        &registry,
        &ChannelObserver { tx: &event_tx },
        &cancel,
    );

    assert!(matches!(
        outcome,
        TurnOutcome::FatalError(ref err) if err == "tool iteration limit exceeded"
    ));
    assert_eq!(
        counter.load(Ordering::SeqCst),
        (MAX_TOOL_ITERATIONS / 2) as u32,
        "duplicate-success guard should block every immediate retry and cut side effects roughly in half"
    );
}

#[test]
fn run_turn_blocks_repeated_identical_successful_tool_calls() {
    struct DuplicateSuccessProvider;

    impl LlmProvider for DuplicateSuccessProvider {
        fn stream_turn(
            &self,
            _model: &str,
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

    let outcome = execute_turn(
        &mut messages,
        &DuplicateSuccessProvider,
        "echo",
        &registry,
        &ChannelObserver { tx: &event_tx },
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
fn run_turn_sends_parse_error_to_model_for_malformed_json() {
    struct MalformedArgsProvider;

    impl LlmProvider for MalformedArgsProvider {
        fn stream_turn(
            &self,
            _model: &str,
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
                // Malformed JSON: unescaped quotes inside string value.
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
    let mut messages = user_messages("test malformed args");

    let outcome = execute_turn(
        &mut messages,
        &MalformedArgsProvider,
        "echo",
        &registry,
        &ChannelObserver { tx: &event_tx },
        &cancel,
    );
    assert!(matches!(outcome, TurnOutcome::Complete));
    // Tool should NOT be called — error returned to model instead.
    assert_eq!(counter.load(Ordering::SeqCst), 0);
    // The error message should be sent back as a tool result.
    assert!(messages.iter().any(|message| {
        matches!(message, ProviderMessage::ToolResult { output, is_error, .. }
                if *is_error && output.contains("JSON PARSE ERROR"))
    }));
}

#[test]
fn run_turn_repairs_control_chars_and_executes_tool() {
    struct ControlCharArgsProvider;

    impl LlmProvider for ControlCharArgsProvider {
        fn stream_turn(
            &self,
            _model: &str,
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
                // Literal newline inside JSON string value (common Qwen pattern).
                let args = "{\"command\": \"echo\nhello\"}";
                let _ = tx.send(ProviderEvent::ToolCall {
                    call_id: "call-1".into(),
                    name: "count".into(),
                    arguments: args.into(),
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
    let mut messages = user_messages("test control char repair");

    let outcome = execute_turn(
        &mut messages,
        &ControlCharArgsProvider,
        "echo",
        &registry,
        &ChannelObserver { tx: &event_tx },
        &cancel,
    );
    assert!(matches!(outcome, TurnOutcome::Complete));
    // Control chars should be repaired and tool should execute.
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    // Repair note should be present in tool result.
    assert!(messages.iter().any(|message| {
        matches!(message, ProviderMessage::ToolResult { output, is_error, .. }
                if !*is_error && output.contains(JSON_REPAIR_NOTE))
    }));
}

struct TimeoutTool {
    calls: Arc<AtomicU32>,
}

impl Tool for TimeoutTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "slow_bash".into(),
            description: "always times out".into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        }
    }

    fn execute(&self, _args: serde_json::Value) -> crate::agent::tools::ToolResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        crate::agent::tools::ToolResult {
            content: vec![crate::agent::tools::ContentPart::text(
                "command timed out after 30s",
            )],
            exit_code: None,
            is_error: true,
            output_truncated: false,
        }
    }
}

#[test]
fn run_turn_adds_timeout_guidance_on_first_timeout_error() {
    struct SingleTimeoutProvider;

    impl LlmProvider for SingleTimeoutProvider {
        fn stream_turn(
            &self,
            _model: &str,
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
                return;
            }

            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-timeout-1".into(),
                name: "slow_bash".into(),
                arguments: "{}".into(),
            });
            let _ = tx.send(ProviderEvent::TurnComplete);
        }
    }

    let calls = Arc::new(AtomicU32::new(0));
    let mut registry = empty_registry();
    registry.register(Box::new(TimeoutTool {
        calls: Arc::clone(&calls),
    }));

    let (event_tx, _event_rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut messages = user_messages("run once");

    let outcome = execute_turn(
        &mut messages,
        &SingleTimeoutProvider,
        "echo",
        &registry,
        &ChannelObserver { tx: &event_tx },
        &cancel,
    );

    assert!(matches!(outcome, TurnOutcome::Complete));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(messages.iter().any(|message| {
        matches!(message, ProviderMessage::ToolResult { output, is_error, .. }
                if *is_error && output.contains("TIMEOUT NOTE"))
    }));
}

#[test]
fn run_turn_escalates_guidance_after_repeated_timeout_errors() {
    struct RepeatedTimeoutProvider;

    impl LlmProvider for RepeatedTimeoutProvider {
        fn stream_turn(
            &self,
            _model: &str,
            messages: &[ProviderMessage],
            _tools: &[ToolSpec],
            tx: Sender<ProviderEvent>,
        ) {
            let last_error = messages.iter().rev().find_map(|message| match message {
                ProviderMessage::ToolResult {
                    output, is_error, ..
                } if *is_error => Some(output.as_str()),
                _ => None,
            });

            if let Some(output) = last_error
                && output.contains("REPEATED TIMEOUT")
            {
                let _ = tx.send(ProviderEvent::TextDelta("done".into()));
                let _ = tx.send(ProviderEvent::TurnComplete);
                return;
            }

            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-timeout-loop".into(),
                name: "slow_bash".into(),
                arguments: "{}".into(),
            });
            let _ = tx.send(ProviderEvent::TurnComplete);
        }
    }

    let calls = Arc::new(AtomicU32::new(0));
    let mut registry = empty_registry();
    registry.register(Box::new(TimeoutTool {
        calls: Arc::clone(&calls),
    }));

    let (event_tx, _event_rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut messages = user_messages("run with retries");

    let outcome = execute_turn(
        &mut messages,
        &RepeatedTimeoutProvider,
        "echo",
        &registry,
        &ChannelObserver { tx: &event_tx },
        &cancel,
    );

    assert!(matches!(outcome, TurnOutcome::Complete));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(messages.iter().any(|message| {
        matches!(message, ProviderMessage::ToolResult { output, is_error, .. }
                if *is_error && output.contains("REPEATED TIMEOUT"))
    }));
}

#[test]
fn run_turn_blocks_repeated_success_for_batched_tool_calls() {
    struct NamedCountingTool {
        name: &'static str,
        calls: Arc<AtomicU32>,
    }

    impl Tool for NamedCountingTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: self.name.to_string(),
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

    struct BatchedDuplicateProvider;

    impl LlmProvider for BatchedDuplicateProvider {
        fn stream_turn(
            &self,
            _model: &str,
            messages: &[ProviderMessage],
            _tools: &[ToolSpec],
            tx: Sender<ProviderEvent>,
        ) {
            let saw_repeat_guard = messages.iter().any(|message| {
                matches!(message, ProviderMessage::ToolResult { output, is_error, .. }
                        if *is_error && output.contains("REPEATED SUCCESS"))
            });

            if saw_repeat_guard {
                let _ = tx.send(ProviderEvent::TextDelta("done".into()));
                let _ = tx.send(ProviderEvent::TurnComplete);
                return;
            }

            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-a".into(),
                name: "count_a".into(),
                arguments: "{}".into(),
            });
            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-b".into(),
                name: "count_b".into(),
                arguments: "{}".into(),
            });
            let _ = tx.send(ProviderEvent::TurnComplete);
        }
    }

    let counter_a = Arc::new(AtomicU32::new(0));
    let counter_b = Arc::new(AtomicU32::new(0));
    let mut registry = empty_registry();
    registry.register(Box::new(NamedCountingTool {
        name: "count_a",
        calls: Arc::clone(&counter_a),
    }));
    registry.register(Box::new(NamedCountingTool {
        name: "count_b",
        calls: Arc::clone(&counter_b),
    }));

    let (event_tx, _event_rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut messages = user_messages("run batch once");

    let outcome = execute_turn(
        &mut messages,
        &BatchedDuplicateProvider,
        "echo",
        &registry,
        &ChannelObserver { tx: &event_tx },
        &cancel,
    );

    assert!(matches!(outcome, TurnOutcome::Complete));
    assert_eq!(counter_a.load(Ordering::SeqCst), 1);
    assert_eq!(counter_b.load(Ordering::SeqCst), 1);
    assert!(messages.iter().any(|message| {
        matches!(message, ProviderMessage::ToolResult { output, is_error, .. }
                if *is_error && output.contains("REPEATED SUCCESS"))
    }));
}

struct TrackedClaimedTool {
    name: &'static str,
    resource: &'static str,
    access: ResourceAccess,
    delay: Duration,
    active_calls: Arc<AtomicU32>,
    max_parallel_calls: Arc<AtomicU32>,
    completion_log: Arc<Mutex<Vec<&'static str>>>,
}

impl Tool for TrackedClaimedTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name.into(),
            description: "tracked claimed tool".into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        }
    }

    fn resource_claims(&self, _args: &serde_json::Value) -> Vec<ResourceClaim> {
        vec![ResourceClaim {
            resource: self.resource.into(),
            access: self.access,
        }]
    }

    fn execute(&self, _args: serde_json::Value) -> crate::agent::tools::ToolResult {
        let active = self.active_calls.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_parallel_calls.fetch_max(active, Ordering::SeqCst);
        std::thread::sleep(self.delay);
        self.active_calls.fetch_sub(1, Ordering::SeqCst);
        self.completion_log
            .lock()
            .expect("log lock")
            .push(self.name);
        crate::agent::tools::ToolResult::success(self.name)
    }
}

#[test]
fn run_turn_executes_non_conflicting_calls_in_parallel_batches_with_stable_result_order() {
    struct ParallelBatchProvider;

    impl LlmProvider for ParallelBatchProvider {
        fn stream_turn(
            &self,
            _model: &str,
            messages: &[ProviderMessage],
            _tools: &[ToolSpec],
            tx: Sender<ProviderEvent>,
        ) {
            let saw_tool_results = messages
                .iter()
                .any(|message| matches!(message, ProviderMessage::ToolResult { .. }));
            if saw_tool_results {
                let _ = tx.send(ProviderEvent::TextDelta("done".into()));
                let _ = tx.send(ProviderEvent::TurnComplete);
                return;
            }

            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-slow".into(),
                name: "slow_read".into(),
                arguments: "{}".into(),
            });
            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-fast".into(),
                name: "fast_read".into(),
                arguments: "{}".into(),
            });
            let _ = tx.send(ProviderEvent::TurnComplete);
        }
    }

    let active_calls = Arc::new(AtomicU32::new(0));
    let max_parallel_calls = Arc::new(AtomicU32::new(0));
    let completion_log = Arc::new(Mutex::new(Vec::new()));

    let mut registry = empty_registry();
    registry.register(Box::new(TrackedClaimedTool {
        name: "slow_read",
        resource: "fs:src/slow.rs",
        access: ResourceAccess::ReadOnly,
        delay: Duration::from_millis(80),
        active_calls: Arc::clone(&active_calls),
        max_parallel_calls: Arc::clone(&max_parallel_calls),
        completion_log: Arc::clone(&completion_log),
    }));
    registry.register(Box::new(TrackedClaimedTool {
        name: "fast_read",
        resource: "fs:src/fast.rs",
        access: ResourceAccess::ReadOnly,
        delay: Duration::from_millis(10),
        active_calls,
        max_parallel_calls: Arc::clone(&max_parallel_calls),
        completion_log: Arc::clone(&completion_log),
    }));

    let (event_tx, event_rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut messages = user_messages("run in parallel");

    let outcome = execute_turn(
        &mut messages,
        &ParallelBatchProvider,
        "echo",
        &registry,
        &ChannelObserver { tx: &event_tx },
        &cancel,
    );

    assert!(matches!(outcome, TurnOutcome::Complete));
    assert_eq!(max_parallel_calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        completion_log.lock().expect("log lock").as_slice(),
        ["fast_read", "slow_read"]
    );

    let tool_results = messages
        .iter()
        .filter_map(|message| match message {
            ProviderMessage::ToolResult { tool_call_id, .. } => Some(tool_call_id.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tool_results, vec!["call-slow", "call-fast"]);

    let completion_events = event_rx
        .try_iter()
        .filter_map(|event| match event {
            AgentEvent::ToolCallCompleted { call_id, .. } => Some(call_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(completion_events, vec!["call-slow", "call-fast"]);
}

#[test]
fn run_turn_serializes_conflicting_calls_into_separate_batches() {
    struct ConflictingBatchProvider;

    impl LlmProvider for ConflictingBatchProvider {
        fn stream_turn(
            &self,
            _model: &str,
            messages: &[ProviderMessage],
            _tools: &[ToolSpec],
            tx: Sender<ProviderEvent>,
        ) {
            let saw_tool_results = messages
                .iter()
                .any(|message| matches!(message, ProviderMessage::ToolResult { .. }));
            if saw_tool_results {
                let _ = tx.send(ProviderEvent::TextDelta("done".into()));
                let _ = tx.send(ProviderEvent::TurnComplete);
                return;
            }

            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-read".into(),
                name: "shared_read".into(),
                arguments: "{}".into(),
            });
            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-write".into(),
                name: "shared_write".into(),
                arguments: "{}".into(),
            });
            let _ = tx.send(ProviderEvent::TurnComplete);
        }
    }

    let active_calls = Arc::new(AtomicU32::new(0));
    let max_parallel_calls = Arc::new(AtomicU32::new(0));
    let completion_log = Arc::new(Mutex::new(Vec::new()));

    let mut registry = empty_registry();
    registry.register(Box::new(TrackedClaimedTool {
        name: "shared_read",
        resource: "fs:src/shared.rs",
        access: ResourceAccess::ReadOnly,
        delay: Duration::from_millis(25),
        active_calls: Arc::clone(&active_calls),
        max_parallel_calls: Arc::clone(&max_parallel_calls),
        completion_log: Arc::clone(&completion_log),
    }));
    registry.register(Box::new(TrackedClaimedTool {
        name: "shared_write",
        resource: "fs:src/shared.rs",
        access: ResourceAccess::ReadWrite,
        delay: Duration::from_millis(25),
        active_calls,
        max_parallel_calls: Arc::clone(&max_parallel_calls),
        completion_log: Arc::clone(&completion_log),
    }));

    let (event_tx, _event_rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut messages = user_messages("serialize conflicts");

    let outcome = execute_turn(
        &mut messages,
        &ConflictingBatchProvider,
        "echo",
        &registry,
        &ChannelObserver { tx: &event_tx },
        &cancel,
    );

    assert!(matches!(outcome, TurnOutcome::Complete));
    assert_eq!(max_parallel_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        completion_log.lock().expect("log lock").as_slice(),
        ["shared_read", "shared_write"]
    );
}

struct CancellingObserver {
    cancel_flag: Arc<AtomicBool>,
    started: Arc<Mutex<Vec<String>>>,
    completed: Arc<Mutex<Vec<String>>>,
}

impl crate::agent::runtime::turn::TurnObserver for CancellingObserver {
    fn on_text_delta(&self, _text: &str) {}

    fn on_thinking_delta(&self, _text: &str) {}

    fn on_tool_call_started(&self, call_id: &str, _tool_name: &str, _arguments: &str) {
        let mut started = self.started.lock().expect("started lock");
        started.push(call_id.to_string());
        if started.len() == 1 {
            self.cancel_flag.store(true, Ordering::SeqCst);
        }
    }

    fn on_tool_call_completed(&self, call_id: &str, _output: &str, _is_error: bool) {
        self.completed
            .lock()
            .expect("completed lock")
            .push(call_id.to_string());
    }

    fn on_turn_failed(&self, _error: &str) {}
}

#[test]
fn run_turn_stops_launching_additional_parallel_calls_after_cancel() {
    struct CancelBatchProvider;

    impl LlmProvider for CancelBatchProvider {
        fn stream_turn(
            &self,
            _model: &str,
            _messages: &[ProviderMessage],
            _tools: &[ToolSpec],
            tx: Sender<ProviderEvent>,
        ) {
            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-1".into(),
                name: "first_read".into(),
                arguments: "{}".into(),
            });
            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-2".into(),
                name: "second_read".into(),
                arguments: "{}".into(),
            });
            let _ = tx.send(ProviderEvent::TurnComplete);
        }
    }

    let first_calls = Arc::new(AtomicU32::new(0));
    let second_calls = Arc::new(AtomicU32::new(0));
    let completion_log = Arc::new(Mutex::new(Vec::new()));

    struct CountingWrapper {
        inner: TrackedClaimedTool,
        calls: Arc<AtomicU32>,
    }

    impl Tool for CountingWrapper {
        fn spec(&self) -> ToolSpec {
            self.inner.spec()
        }

        fn resource_claims(&self, args: &serde_json::Value) -> Vec<ResourceClaim> {
            self.inner.resource_claims(args)
        }

        fn execute(&self, args: serde_json::Value) -> crate::agent::tools::ToolResult {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.inner.execute(args)
        }
    }

    let mut registry = empty_registry();
    registry.register(Box::new(CountingWrapper {
        inner: TrackedClaimedTool {
            name: "first_read",
            resource: "fs:src/first.rs",
            access: ResourceAccess::ReadOnly,
            delay: Duration::from_millis(20),
            active_calls: Arc::new(AtomicU32::new(0)),
            max_parallel_calls: Arc::new(AtomicU32::new(0)),
            completion_log: Arc::clone(&completion_log),
        },
        calls: Arc::clone(&first_calls),
    }));
    registry.register(Box::new(CountingWrapper {
        inner: TrackedClaimedTool {
            name: "second_read",
            resource: "fs:src/second.rs",
            access: ResourceAccess::ReadOnly,
            delay: Duration::from_millis(20),
            active_calls: Arc::new(AtomicU32::new(0)),
            max_parallel_calls: Arc::new(AtomicU32::new(0)),
            completion_log: Arc::clone(&completion_log),
        },
        calls: Arc::clone(&second_calls),
    }));

    let cancel = Arc::new(AtomicBool::new(false));
    let started = Arc::new(Mutex::new(Vec::new()));
    let completed = Arc::new(Mutex::new(Vec::new()));
    let observer = CancellingObserver {
        cancel_flag: Arc::clone(&cancel),
        started: Arc::clone(&started),
        completed: Arc::clone(&completed),
    };
    let mut messages = user_messages("cancel parallel batch");

    let outcome = execute_turn(
        &mut messages,
        &CancelBatchProvider,
        "echo",
        &registry,
        &observer,
        &cancel,
    );

    assert!(matches!(outcome, TurnOutcome::Cancelled));
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert_eq!(second_calls.load(Ordering::SeqCst), 0);
    assert_eq!(started.lock().expect("started lock").as_slice(), ["call-1"]);
    assert_eq!(
        completed.lock().expect("completed lock").as_slice(),
        ["call-1"]
    );
    assert_eq!(
        messages
            .iter()
            .filter_map(|message| match message {
                ProviderMessage::ToolResult { tool_call_id, .. } => Some(tool_call_id.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec!["call-1"]
    );
}

#[test]
fn provider_that_sends_no_terminal_event_gets_auto_complete() {
    struct NoTerminalProvider;

    impl LlmProvider for NoTerminalProvider {
        fn stream_turn(
            &self,
            _model: &str,
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
        "echo",
        &user_messages("hi"),
        &[],
        &ChannelObserver { tx: &event_tx },
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

// ---- parse_or_repair_json tests ----

#[test]
fn parse_or_repair_succeeds_on_valid_json() {
    let raw = r#"{"command": "ls"}"#;
    let parsed = parse_or_repair_json(raw).expect("should parse");
    assert!(!parsed.was_repaired);
    assert_eq!(parsed.value["command"], "ls");
}

#[test]
fn parse_or_repair_returns_error_for_unescaped_quotes() {
    // Unescaped quotes are now sent to model as error (Codex-style approach).
    let raw = r#"{"path": "hello.py", "content": "print("hello world!\")"}"#;
    let result = parse_or_repair_json(raw);
    assert!(result.is_err());
}

#[test]
fn parse_or_repair_returns_error_for_garbage() {
    let result = parse_or_repair_json("total garbage");
    assert!(result.is_err());
}

// ---- extract_json_payload tests ----

#[test]
fn extract_json_strips_trailing_tool_call_tag() {
    let raw = r#"{"path": "a.py", "content": "x = 1"}</tool_call>"#;
    let extracted = extract_json_payload(raw).expect("should extract");
    assert_eq!(extracted, r#"{"path": "a.py", "content": "x = 1"}"#);
}

#[test]
fn extract_json_strips_leading_junk() {
    let raw = r#"some text before {"key": "val"}"#;
    let extracted = extract_json_payload(raw).expect("should extract");
    assert_eq!(extracted, r#"{"key": "val"}"#);
}

#[test]
fn extract_json_handles_nested_braces() {
    let raw = r#"{"a": {"b": 1}} trailing"#;
    let extracted = extract_json_payload(raw).expect("should extract");
    assert_eq!(extracted, r#"{"a": {"b": 1}}"#);
}

#[test]
fn extract_json_ignores_braces_inside_strings() {
    let raw = r#"{"content": "func() { return }"} junk"#;
    let extracted = extract_json_payload(raw).expect("should extract");
    let val: Value = serde_json::from_str(extracted).expect("valid JSON");
    assert_eq!(val["content"], "func() { return }");
}

#[test]
fn extract_json_returns_none_for_exact_json() {
    let raw = r#"{"key": "val"}"#;
    // No trimming needed, returns None to signal "use raw as-is".
    assert!(extract_json_payload(raw).is_none());
}

#[test]
fn extract_json_handles_array() {
    let raw = r#"[1, 2, 3] extra"#;
    let extracted = extract_json_payload(raw).expect("should extract");
    assert_eq!(extracted, "[1, 2, 3]");
}

#[test]
fn parse_or_repair_recovers_from_control_chars() {
    let raw = "{\"path\": \"fib.py\", \"content\": \"line1\nline2\"}";
    let parsed = parse_or_repair_json(raw).expect("should recover");
    assert!(parsed.was_repaired);
}

#[test]
fn parse_or_repair_recovers_trailing_tool_call_tag() {
    let raw = r#"{"command": "ls -la"}</tool_call>"#;
    let parsed = parse_or_repair_json(raw).expect("should recover via extraction");
    assert!(parsed.was_repaired);
    assert_eq!(parsed.value["command"], "ls -la");
}

// ---- repair_control_chars tests ----

#[test]
fn repair_control_chars_fixes_literal_newlines() {
    let raw = "{\"path\": \"fib.py\", \"content\": \"def f(n):\n    return n\"}";
    let repaired = repair_control_chars(raw).expect("should repair");
    let val: Value = serde_json::from_str(&repaired).expect("valid JSON");
    assert_eq!(val["content"], "def f(n):\n    return n");
}

#[test]
fn repair_control_chars_returns_none_for_clean_json() {
    let raw = r#"{"content": "no control chars here"}"#;
    assert!(repair_control_chars(raw).is_none());
}

#[test]
fn repair_control_chars_preserves_structural_newlines() {
    let raw = "{\n  \"key\": \"value\"\n}";
    assert!(repair_control_chars(raw).is_none());
}

#[test]
fn new_surfaces_spawn_errors_instead_of_panicking() {
    let provider = crate::provider::echo::EchoProvider::new(0);
    let runtime = AgentRuntime::new_with_spawner(
        Box::new(provider),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        "echo".to_owned(),
        |_worker| Err(std::io::Error::other("spawn failed")),
    );

    assert!(
        runtime.is_err(),
        "spawn failures should return an explicit error"
    );
    let err = runtime.err().expect("runtime should carry a spawn error");
    assert!(matches!(err, AgentRuntimeError::ThreadSpawn(_)));
}
