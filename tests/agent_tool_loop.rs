use blazar::agent::protocol::AgentEvent;
use blazar::agent::runtime::AgentRuntime;
use blazar::agent::tools::ToolSpec;
use blazar::provider::{LlmProvider, ProviderEvent, ProviderMessage};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn fresh_workspace() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-workspaces")
        .join(format!("blazar-tool-loop-{suffix}"));
    fs::create_dir_all(&path).unwrap();
    path
}

fn collect_events(runtime: &AgentRuntime, timeout: Duration) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if let Some(event) = runtime.try_recv() {
            let done = matches!(
                event,
                AgentEvent::TurnComplete | AgentEvent::TurnFailed { .. }
            );
            events.push(event);
            if done {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    events
}

struct ToolLoopProvider {
    calls: Arc<AtomicU32>,
}

impl LlmProvider for ToolLoopProvider {
    fn stream_turn(
        &self,
        _model: &str,
        messages: &[ProviderMessage],
        _tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        let call_index = self.calls.fetch_add(1, Ordering::SeqCst);
        if call_index == 0 {
            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-1".into(),
                name: "read_file".into(),
                arguments: json!({ "path": "notes.txt" }).to_string(),
            });
            let _ = tx.send(ProviderEvent::TurnComplete);
            return;
        }

        let tool_output = messages
            .iter()
            .find_map(|message| match message {
                ProviderMessage::ToolResult { output, .. } => Some(output.clone()),
                _ => None,
            })
            .expect("tool result should be replayed into second provider pass");

        let _ = tx.send(ProviderEvent::TextDelta(format!("Saw: {tool_output}")));
        let _ = tx.send(ProviderEvent::TurnComplete);
    }
}

#[test]
fn runtime_executes_tool_call_and_resumes_generation() {
    let workspace = fresh_workspace();
    fs::write(workspace.join("notes.txt"), "tool result body").unwrap();

    let runtime = AgentRuntime::new(
        Box::new(ToolLoopProvider {
            calls: Arc::new(AtomicU32::new(0)),
        }),
        workspace,
        "echo".to_owned(),
    )
    .expect("runtime should initialize");

    runtime.submit_turn("read the file").unwrap();
    let events = collect_events(&runtime, Duration::from_secs(2));

    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::ToolCallStarted { tool_name, .. } if tool_name == "read_file"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::ToolCallCompleted {
            is_error: false,
            ..
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::TextDelta { text } if text.contains("tool result body")
    )));
}

struct BatchedToolLoopProvider {
    calls: Arc<AtomicU32>,
}

impl LlmProvider for BatchedToolLoopProvider {
    fn stream_turn(
        &self,
        _model: &str,
        messages: &[ProviderMessage],
        _tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        let call_index = self.calls.fetch_add(1, Ordering::SeqCst);
        if call_index == 0 {
            let _ = tx.send(ProviderEvent::TextDelta("Checking files".into()));
            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-1".into(),
                name: "read_file".into(),
                arguments: json!({ "path": "a.txt" }).to_string(),
            });
            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-2".into(),
                name: "read_file".into(),
                arguments: json!({ "path": "b.txt" }).to_string(),
            });
            let _ = tx.send(ProviderEvent::TurnComplete);
            return;
        }

        let replay_is_grouped = matches!(
            messages,
            [
                ProviderMessage::User { .. },
                ProviderMessage::Assistant { content },
                ProviderMessage::ToolCall { id: first_id, .. },
                ProviderMessage::ToolCall { id: second_id, .. },
                ProviderMessage::ToolResult { tool_call_id: first_result, output: first_output, .. },
                ProviderMessage::ToolResult { tool_call_id: second_result, output: second_output, .. },
            ] if content == "Checking files"
                && first_id == "call-1"
                && second_id == "call-2"
                && first_result == "call-1"
                && second_result == "call-2"
                && first_output == "alpha"
                && second_output == "beta"
        );

        if !replay_is_grouped {
            let _ = tx.send(ProviderEvent::Error(
                "tool replay order should batch tool calls before tool results".into(),
            ));
            return;
        }

        let _ = tx.send(ProviderEvent::TextDelta("Saw alpha and beta".into()));
        let _ = tx.send(ProviderEvent::TurnComplete);
    }
}

#[test]
fn runtime_batches_tool_calls_before_replaying_tool_results() {
    let workspace = fresh_workspace();
    fs::write(workspace.join("a.txt"), "alpha").unwrap();
    fs::write(workspace.join("b.txt"), "beta").unwrap();

    let runtime = AgentRuntime::new(
        Box::new(BatchedToolLoopProvider {
            calls: Arc::new(AtomicU32::new(0)),
        }),
        workspace,
        "echo".to_owned(),
    )
    .expect("runtime should initialize");

    runtime.submit_turn("compare both files").unwrap();
    let events = collect_events(&runtime, Duration::from_secs(2));

    assert!(
        events
            .iter()
            .all(|event| !matches!(event, AgentEvent::TurnFailed { .. })),
        "replayed tool history should not fail"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::TextDelta { text } if text.contains("alpha and beta")
    )));
}
