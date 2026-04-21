use super::*;
use crate::chat::view::render_to_lines_for_test;
use crate::provider::{ProviderEvent, ProviderMessage};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct UnicodeArgumentProvider;

impl LlmProvider for UnicodeArgumentProvider {
    fn stream_turn(
        &self,
        messages: &[ProviderMessage],
        _tools: &[crate::agent::tools::ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        let has_tool_result = messages
            .iter()
            .any(|message| matches!(message, ProviderMessage::ToolResult { .. }));

        if has_tool_result {
            let _ = tx.send(ProviderEvent::TurnComplete);
            return;
        }

        let _ = tx.send(ProviderEvent::ToolCall {
            call_id: "call-1".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({
                "path": "😀".repeat(20)
            })
            .to_string(),
        });
        let _ = tx.send(ProviderEvent::TurnComplete);
    }
}

struct UnicodeOutputProvider;

impl LlmProvider for UnicodeOutputProvider {
    fn stream_turn(
        &self,
        messages: &[ProviderMessage],
        _tools: &[crate::agent::tools::ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        let has_tool_result = messages
            .iter()
            .any(|message| matches!(message, ProviderMessage::ToolResult { .. }));

        if has_tool_result {
            let _ = tx.send(ProviderEvent::TurnComplete);
            return;
        }

        let _ = tx.send(ProviderEvent::ToolCall {
            call_id: "call-1".into(),
            name: "bash".into(),
            arguments: serde_json::json!({
                "command": "printf '\\U1F600%.0s' {1..20}"
            })
            .to_string(),
        });
        let _ = tx.send(ProviderEvent::TurnComplete);
    }
}

struct CapturePromptProvider {
    prompt: Arc<Mutex<Option<String>>>,
}

impl LlmProvider for CapturePromptProvider {
    fn stream_turn(
        &self,
        messages: &[ProviderMessage],
        _tools: &[crate::agent::tools::ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        let prompt = messages
            .iter()
            .rev()
            .find_map(|message| match message {
                ProviderMessage::User { content } => Some(content.clone()),
                _ => None,
            })
            .expect("provider should receive the user prompt");

        *self.prompt.lock().expect("prompt mutex poisoned") = Some(prompt);
        let _ = tx.send(ProviderEvent::TurnComplete);
    }
}

#[test]
fn tick_handles_multibyte_tool_arguments_without_panicking() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.agent_runtime =
        AgentRuntime::new(Box::new(UnicodeArgumentProvider), PathBuf::from(repo_path))
            .expect("runtime should initialize");

    app.agent_runtime.submit_turn("read unicode path").unwrap();
    std::thread::sleep(Duration::from_millis(100));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.tick()));

    assert!(
        result.is_ok(),
        "tick should not panic on multibyte tool arguments"
    );
}

#[test]
fn tick_handles_multibyte_tool_output_without_panicking() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.agent_runtime =
        AgentRuntime::new(Box::new(UnicodeOutputProvider), PathBuf::from(repo_path))
            .expect("runtime should initialize");

    app.agent_runtime
        .submit_turn("render unicode output")
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.tick()));

    assert!(
        result.is_ok(),
        "tick should not panic on multibyte tool output"
    );
}

#[test]
fn send_message_queues_when_agent_busy() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    // First message dispatches normally
    app.send_message("first message");
    assert!(app.pending_messages.is_empty());

    // Simulate agent becoming busy
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "t1".into(),
    });
    assert!(app.agent_state.is_busy());

    // Second and third messages should be queued, not dropped
    app.send_message("second message");
    app.send_message("third message");
    assert_eq!(app.pending_messages.len(), 2);
    assert_eq!(app.pending_messages[0].user_text, "second message");
    assert_eq!(app.pending_messages[1].user_text, "third message");

    // Both still appear in timeline (UI not lost)
    let user_messages: Vec<&str> = app
        .timeline
        .iter()
        .filter(|e| e.actor == Actor::User)
        .map(|e| e.body.as_str())
        .collect();
    assert!(user_messages.contains(&"second message"));
    assert!(user_messages.contains(&"third message"));
}

#[test]
fn dispatch_next_queued_drains_fifo_on_turn_complete() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    // Queue two messages
    app.pending_messages
        .push_back(build_pending_turn("queued-a"));
    app.pending_messages
        .push_back(build_pending_turn("queued-b"));

    // Simulate TurnComplete — should dispatch first queued message
    app.apply_agent_event_for_test(AgentEvent::TurnComplete);
    assert_eq!(app.pending_messages.len(), 1);
    assert_eq!(app.pending_messages[0].user_text, "queued-b");
}

#[test]
fn dispatch_next_queued_drains_on_turn_failed() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.pending_messages
        .push_back(build_pending_turn("queued-after-fail"));

    // TurnFailed should also drain the queue
    app.apply_agent_event_for_test(AgentEvent::TurnFailed {
        error: "test error".into(),
    });
    assert!(app.pending_messages.is_empty());
}

#[test]
fn paste_action_inserts_into_composer_without_submitting() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    let pasted = "line one\nline two\nline three";
    app.handle_action(InputAction::Paste(pasted.to_owned()));

    let text = app.composer_text();
    assert!(text.contains("line one"));
    assert!(text.contains("line two"));
    assert!(text.contains("line three"));
    // No message was sent — timeline should only have the welcome message
    let user_msgs: Vec<_> = app
        .timeline
        .iter()
        .filter(|e| e.actor == Actor::User)
        .collect();
    assert!(user_msgs.is_empty(), "paste should not auto-submit");
}

#[test]
fn plan_command_rewrites_prompt_for_planning_mode() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    let captured_prompt = Arc::new(Mutex::new(None));
    app.agent_runtime = AgentRuntime::new(
        Box::new(CapturePromptProvider {
            prompt: captured_prompt.clone(),
        }),
        PathBuf::from(repo_path),
    )
    .expect("runtime should initialize");

    app.send_message("/plan add minimax provider");
    std::thread::sleep(Duration::from_millis(50));

    let prompt = captured_prompt
        .lock()
        .expect("prompt mutex poisoned")
        .clone()
        .expect("provider should capture a prompt");

    assert!(prompt.contains("planning mode"));
    assert!(prompt.contains("First line must be a short plain-text title"));
    assert!(prompt.contains("add minimax provider"));
}

#[test]
fn planning_turn_uses_thinking_while_streaming_then_sets_title() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.send_message("/plan add minimax provider");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "plan-1".into(),
    });

    let streaming_lines = render_to_lines_for_test(&mut app, 90, 24);
    let streaming_text = streaming_lines.join("\n");
    assert!(
        streaming_text.contains("thinking"),
        "planning turns should show thinking while streaming"
    );

    app.apply_agent_event_for_test(AgentEvent::TextDelta {
            text: "MiniMax Provider Integration\n\n1. Review current provider abstraction\n2. Add provider config\n".into(),
        });
    app.apply_agent_event_for_test(AgentEvent::TurnComplete);

    let assistant_entry = app
        .timeline
        .iter()
        .rev()
        .find(|entry| entry.actor == Actor::Assistant && entry.kind == EntryKind::Message)
        .expect("assistant response entry should exist");

    assert_eq!(
        assistant_entry.title.as_deref(),
        Some("MiniMax Provider Integration")
    );
    assert_eq!(
        assistant_entry.body,
        "1. Review current provider abstraction\n2. Add provider config"
    );

    let completed_lines = render_to_lines_for_test(&mut app, 90, 24);
    let completed_text = completed_lines.join("\n");
    assert!(completed_text.contains("MiniMax Provider Integration"));
    assert!(!completed_text.contains("Blazar #2"));
}

#[test]
fn follow_up_turn_reuses_latest_plan_title_while_streaming() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline.push(
        TimelineEntry::response("1. Review current provider abstraction")
            .with_title("MiniMax Provider Integration"),
    );
    app.active_turn_kind = Some(TurnKind::Chat);
    app.active_turn_title = Some("MiniMax Provider Integration".into());
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "exec-1".into(),
    });

    let lines = render_to_lines_for_test(&mut app, 90, 24);
    let text = lines.join("\n");
    assert!(text.contains("MiniMax Provider Integration"));
    assert!(!text.contains("streaming…"));
}
