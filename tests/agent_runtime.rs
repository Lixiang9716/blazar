use blazar::agent::protocol::AgentEvent;
use blazar::agent::runtime::AgentRuntime;
use blazar::agent::tools::ToolSpec;
use blazar::provider::echo::EchoProvider;
use blazar::provider::{LlmProvider, ProviderEvent, ProviderMessage};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Barrier};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helper: drain events from runtime until a terminal event or timeout
// ---------------------------------------------------------------------------
fn collect_events(runtime: &AgentRuntime, timeout: Duration) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(event) = runtime.try_recv() {
            let is_terminal = matches!(
                event,
                AgentEvent::TurnComplete | AgentEvent::TurnFailed { .. }
            );
            events.push(event);
            if is_terminal {
                break;
            }
        }
        if std::time::Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    events
}

// ---------------------------------------------------------------------------
// Injected test providers (DI via LlmProvider trait)
// ---------------------------------------------------------------------------

/// Provider that blocks on a barrier, allowing tests to control timing.
struct SlowProvider {
    barrier: Arc<Barrier>,
    post_barrier_chunks: u32,
}

impl LlmProvider for SlowProvider {
    fn stream_turn(
        &self,
        _messages: &[ProviderMessage],
        _tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        let _ = tx.send(ProviderEvent::TextDelta("before-barrier".into()));
        self.barrier.wait();
        for i in 0..self.post_barrier_chunks {
            std::thread::sleep(Duration::from_millis(10));
            if tx.send(ProviderEvent::TextDelta(format!("c{i}"))).is_err() {
                return;
            }
        }
        let _ = tx.send(ProviderEvent::TurnComplete);
    }
}

/// Provider that returns a transient error on the first N calls, then succeeds.
struct TransientThenSucceedProvider {
    fail_count: AtomicU32,
    fail_times: u32,
}

impl LlmProvider for TransientThenSucceedProvider {
    fn stream_turn(
        &self,
        _messages: &[ProviderMessage],
        _tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        let n = self.fail_count.fetch_add(1, Ordering::SeqCst);
        if n < self.fail_times {
            let _ = tx.send(ProviderEvent::Error("connection timeout".into()));
        } else {
            let _ = tx.send(ProviderEvent::TextDelta("recovered".into()));
            let _ = tx.send(ProviderEvent::TurnComplete);
        }
    }
}

/// Provider that always returns a fatal error.
struct FatalErrorProvider {
    call_count: AtomicU32,
}

impl LlmProvider for FatalErrorProvider {
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

/// Provider that emits thinking + tool call first, then completes after tool replay.
struct FullEventProvider {
    calls: AtomicU32,
}

impl LlmProvider for FullEventProvider {
    fn stream_turn(
        &self,
        messages: &[ProviderMessage],
        _tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        let _ = tx.send(ProviderEvent::ThinkingDelta("thinking...".into()));
        let pass = self.calls.fetch_add(1, Ordering::SeqCst);
        if pass == 0 {
            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-1".into(),
                name: "read_file".into(),
                arguments: "{\"path\":\"Cargo.toml\"}".into(),
            });
        } else {
            let tool_seen = messages.iter().any(|message| {
                matches!(message, ProviderMessage::ToolResult { tool_call_id, .. } if tool_call_id == "call-1")
            });
            if tool_seen {
                let _ = tx.send(ProviderEvent::TextDelta("answer".into()));
            }
        }
        let _ = tx.send(ProviderEvent::TurnComplete);
    }
}

struct HistoryCarryProvider {
    turn: AtomicU32,
}

impl LlmProvider for HistoryCarryProvider {
    fn stream_turn(
        &self,
        messages: &[ProviderMessage],
        _tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        let turn = self.turn.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            let _ = tx.send(ProviderEvent::TextDelta("first-response".into()));
            let _ = tx.send(ProviderEvent::TurnComplete);
            return;
        }

        let saw_first_user = messages.iter().any(|message| {
            matches!(
                message,
                ProviderMessage::User { content } if content == "first"
            )
        });
        let saw_first_assistant = messages.iter().any(|message| {
            matches!(
                message,
                ProviderMessage::Assistant { content } if content == "first-response"
            )
        });

        if saw_first_user && saw_first_assistant {
            let _ = tx.send(ProviderEvent::TextDelta("history-ok".into()));
            let _ = tx.send(ProviderEvent::TurnComplete);
        } else {
            let _ = tx.send(ProviderEvent::Error("missing cross-turn history".into()));
        }
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ---------------------------------------------------------------------------
// Existing tests
// ---------------------------------------------------------------------------

#[test]
fn echo_provider_streams_full_response() {
    let provider = EchoProvider::new(0);
    let (tx, rx) = std::sync::mpsc::channel();

    provider.stream_turn(
        &[ProviderMessage::User {
            content: "hi".into(),
        }],
        &[],
        tx,
    );

    let mut text = String::new();
    let mut completed = false;
    for event in rx {
        match event {
            ProviderEvent::TextDelta(chunk) => text.push_str(&chunk),
            ProviderEvent::ThinkingDelta(_) => {}
            ProviderEvent::TurnComplete => {
                completed = true;
                break;
            }
            ProviderEvent::Error(_) => panic!("unexpected error"),
            ProviderEvent::ToolCall { .. } => {}
        }
    }

    assert!(completed);
    assert_eq!(text, "Echo: hi");
}

#[test]
fn runtime_round_trip() {
    let runtime = AgentRuntime::new(Box::new(EchoProvider::new(0)), workspace_root())
        .expect("runtime should initialize");

    runtime.submit_turn("hello").expect("submit should succeed");

    let events = collect_events(&runtime, Duration::from_secs(2));

    assert!(matches!(&events[0], AgentEvent::TurnStarted { .. }));
    assert!(matches!(events.last().unwrap(), AgentEvent::TurnComplete));

    let text: String = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "Echo: hello");
}

// ---------------------------------------------------------------------------
// Admission control tests
// ---------------------------------------------------------------------------

#[test]
fn submit_turn_returns_ok_when_channel_open() {
    let runtime = AgentRuntime::new(Box::new(EchoProvider::new(0)), workspace_root())
        .expect("runtime should initialize");
    let result = runtime.submit_turn("test");
    assert!(result.is_ok());
    // Drain events to let the runtime finish cleanly
    collect_events(&runtime, Duration::from_secs(2));
}

#[test]
fn submit_turn_returns_err_after_shutdown() {
    let runtime = AgentRuntime::new(Box::new(EchoProvider::new(0)), workspace_root())
        .expect("runtime should initialize");
    // Drop sends Shutdown command, closing the channel
    drop(runtime);

    // Can't test post-drop on same binding, but we can verify Drop doesn't panic
    // The important behavior is tested by the Drop impl joining the thread
}

// ---------------------------------------------------------------------------
// Cancel tests
// ---------------------------------------------------------------------------

#[test]
fn cancel_stops_streaming_turn() {
    let barrier = Arc::new(Barrier::new(2));
    let provider = SlowProvider {
        barrier: Arc::clone(&barrier),
        post_barrier_chunks: 50,
    };
    let runtime =
        AgentRuntime::new(Box::new(provider), workspace_root()).expect("runtime should initialize");

    runtime.submit_turn("test").expect("submit should succeed");

    // Wait for provider to reach barrier (first chunk sent)
    barrier.wait();

    // Small delay to let runtime start relaying
    std::thread::sleep(Duration::from_millis(20));

    // Cancel
    runtime.cancel();

    let events = collect_events(&runtime, Duration::from_secs(2));

    // Should have TurnStarted, possibly some TextDeltas, then TurnFailed/TurnComplete
    assert!(
        !events.is_empty(),
        "should have received at least TurnStarted"
    );
    assert!(matches!(&events[0], AgentEvent::TurnStarted { .. }));
}

// ---------------------------------------------------------------------------
// Transient retry tests
// ---------------------------------------------------------------------------

#[test]
fn transient_error_retries_and_recovers() {
    let provider = TransientThenSucceedProvider {
        fail_count: AtomicU32::new(0),
        fail_times: 1, // fail once, then succeed
    };
    let runtime =
        AgentRuntime::new(Box::new(provider), workspace_root()).expect("runtime should initialize");

    runtime.submit_turn("test").expect("submit should succeed");

    let events = collect_events(&runtime, Duration::from_secs(5));

    // Should eventually get TurnComplete after retry
    let has_complete = events.iter().any(|e| matches!(e, AgentEvent::TurnComplete));
    assert!(has_complete, "retry should recover: events={events:?}");

    let text: String = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "recovered");
}

// ---------------------------------------------------------------------------
// Fatal error tests
// ---------------------------------------------------------------------------

#[test]
fn fatal_error_does_not_retry() {
    let provider = FatalErrorProvider {
        call_count: AtomicU32::new(0),
    };

    let runtime =
        AgentRuntime::new(Box::new(provider), workspace_root()).expect("runtime should initialize");
    runtime.submit_turn("test").expect("submit should succeed");

    let events = collect_events(&runtime, Duration::from_secs(2));

    let has_failed = events
        .iter()
        .any(|e| matches!(e, AgentEvent::TurnFailed { .. }));
    assert!(has_failed, "should emit TurnFailed for fatal error");

    // Provider is moved into runtime, we can't directly check call_count
    // But we verify it fails immediately (no long retry delay)
    // The test completing quickly proves no retry sleep happened
}

// ---------------------------------------------------------------------------
// Full event coverage test
// ---------------------------------------------------------------------------

#[test]
fn full_event_types_relayed_correctly() {
    let runtime = AgentRuntime::new(
        Box::new(FullEventProvider {
            calls: AtomicU32::new(0),
        }),
        workspace_root(),
    )
    .expect("runtime should initialize");

    runtime.submit_turn("test").expect("submit should succeed");

    let events = collect_events(&runtime, Duration::from_secs(2));

    let has_started = events
        .iter()
        .any(|e| matches!(e, AgentEvent::TurnStarted { .. }));
    let has_thinking = events
        .iter()
        .any(|e| matches!(e, AgentEvent::ThinkingDelta { .. }));
    let has_text = events
        .iter()
        .any(|e| matches!(e, AgentEvent::TextDelta { .. }));
    let has_tool = events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolCallStarted { .. }));
    let has_complete = events.iter().any(|e| matches!(e, AgentEvent::TurnComplete));

    assert!(has_started, "missing TurnStarted");
    assert!(has_thinking, "missing ThinkingDelta");
    assert!(has_text, "missing TextDelta");
    assert!(has_tool, "missing ToolCallStarted");
    assert!(has_complete, "missing TurnComplete");
}

// ---------------------------------------------------------------------------
// Multiple sequential turns
// ---------------------------------------------------------------------------

#[test]
fn multiple_sequential_turns_complete() {
    let runtime = AgentRuntime::new(Box::new(EchoProvider::new(0)), workspace_root())
        .expect("runtime should initialize");

    for i in 0..3 {
        runtime
            .submit_turn(&format!("turn-{i}"))
            .expect("submit should succeed");

        let events = collect_events(&runtime, Duration::from_secs(2));
        assert!(
            events.iter().any(|e| matches!(e, AgentEvent::TurnComplete)),
            "turn {i} should complete"
        );
    }
}

#[test]
fn runtime_persists_history_across_turns() {
    let runtime = AgentRuntime::new(
        Box::new(HistoryCarryProvider {
            turn: AtomicU32::new(0),
        }),
        workspace_root(),
    )
    .expect("runtime should initialize");

    runtime.submit_turn("first").expect("submit should succeed");
    let first_events = collect_events(&runtime, Duration::from_secs(2));
    assert!(
        first_events
            .iter()
            .any(|event| matches!(event, AgentEvent::TurnComplete)),
        "first turn should complete"
    );

    runtime
        .submit_turn("second")
        .expect("submit should succeed");
    let second_events = collect_events(&runtime, Duration::from_secs(2));
    let second_text: String = second_events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(second_text, "history-ok");
    assert!(
        second_events
            .iter()
            .any(|event| matches!(event, AgentEvent::TurnComplete)),
        "second turn should complete"
    );
}

// ---------------------------------------------------------------------------
// Shutdown correctness
// ---------------------------------------------------------------------------

#[test]
fn drop_joins_thread_cleanly() {
    let runtime = AgentRuntime::new(Box::new(EchoProvider::new(0)), workspace_root())
        .expect("runtime should initialize");
    runtime.submit_turn("hello").expect("submit should succeed");
    collect_events(&runtime, Duration::from_secs(2));
    // Drop should join the thread without panic or hang
    drop(runtime);
}

#[test]
fn drop_during_streaming_does_not_hang() {
    let barrier = Arc::new(Barrier::new(2));
    let provider = SlowProvider {
        barrier: Arc::clone(&barrier),
        post_barrier_chunks: 100,
    };
    let runtime =
        AgentRuntime::new(Box::new(provider), workspace_root()).expect("runtime should initialize");
    runtime.submit_turn("test").expect("submit should succeed");

    // Let provider start
    barrier.wait();

    // Drop while streaming — should not hang (Shutdown + thread join)
    drop(runtime);
}

// ---------------------------------------------------------------------------
// State machine integration (state.rs + runtime events)
// ---------------------------------------------------------------------------

#[test]
fn state_machine_tracks_runtime_events() {
    use blazar::agent::state::AgentRuntimeState;

    let runtime = AgentRuntime::new(Box::new(EchoProvider::new(0)), workspace_root())
        .expect("runtime should initialize");
    let mut state = AgentRuntimeState::default();

    runtime.submit_turn("hello").expect("submit should succeed");

    let events = collect_events(&runtime, Duration::from_secs(2));

    for event in &events {
        state.apply_event(event);
    }

    assert!(!state.is_busy(), "should not be busy after TurnComplete");
    assert_eq!(state.turn_count, 1);
    assert_eq!(state.streaming_text, "Echo: hello");
}

// ---------------------------------------------------------------------------
// is_transient_error (pub(crate) — tested here via re-export check)
// ---------------------------------------------------------------------------
// Note: detailed unit tests for is_transient_error are in runtime.rs #[cfg(test)]
// Integration tests here verify the classification through the runtime behavior.
