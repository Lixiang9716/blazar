use super::*;
use crate::agent::tools::{ToolCompatibilityTier, ToolRegistry};
use crate::chat::view::render_to_lines_for_test;
use crate::provider::{LlmProvider, ProviderEvent, ProviderMessage};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct UnicodeArgumentProvider;

impl LlmProvider for UnicodeArgumentProvider {
    fn stream_turn(
        &self,
        _model: &str,
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
        _model: &str,
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
        _model: &str,
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

const TEST_WAIT_TIMEOUT: Duration = Duration::from_secs(2);

fn new_test_app() -> ChatApp {
    ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("test app should initialize")
}

#[test]
fn chat_runtime_tool_registry_reports_bridge_compatibility_tier_for_bash() {
    let registry = ToolRegistry::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    assert_eq!(
        registry.compatibility_tier("bash"),
        Some(ToolCompatibilityTier::CompatibilityBridge)
    );
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    condition()
}

#[test]
fn tick_handles_multibyte_tool_arguments_without_panicking() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = new_test_app();
    app.agent_runtime = AgentRuntime::new(
        Box::new(UnicodeArgumentProvider),
        PathBuf::from(repo_path),
        "echo".to_owned(),
    )
    .expect("runtime should initialize");

    app.agent_runtime
        .submit_turn("read unicode path")
        .expect("turn submission should succeed");

    let mut panicked = false;
    let observed = wait_until(TEST_WAIT_TIMEOUT, || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.tick()));
        if result.is_err() {
            panicked = true;
            return true;
        }
        app.timeline
            .iter()
            .any(|entry| matches!(entry.kind, EntryKind::ToolCall { .. }))
    });

    assert!(
        !panicked,
        "tick should not panic on multibyte tool arguments"
    );
    assert!(observed, "tool call event was not observed within timeout");
}

#[test]
fn tick_handles_multibyte_tool_output_without_panicking() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = new_test_app();
    app.agent_runtime = AgentRuntime::new(
        Box::new(UnicodeOutputProvider),
        PathBuf::from(repo_path),
        "echo".to_owned(),
    )
    .expect("runtime should initialize");

    app.agent_runtime
        .submit_turn("render unicode output")
        .expect("turn submission should succeed");

    let mut panicked = false;
    let observed = wait_until(TEST_WAIT_TIMEOUT, || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.tick()));
        if result.is_err() {
            panicked = true;
            return true;
        }
        app.timeline.iter().any(|entry| {
            matches!(
                entry.kind,
                EntryKind::ToolCall {
                    status: ToolCallStatus::Success,
                    ..
                }
            )
        })
    });

    assert!(!panicked, "tick should not panic on multibyte tool output");
    assert!(
        observed,
        "tool completion event was not observed within timeout"
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
        kind: crate::agent::runtime::RuntimeErrorKind::ProviderFatal,
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
    let mut app = new_test_app();
    let captured_prompt = Arc::new(Mutex::new(None));
    let repo_path = env!("CARGO_MANIFEST_DIR");
    app.agent_runtime = AgentRuntime::new(
        Box::new(CapturePromptProvider {
            prompt: captured_prompt.clone(),
        }),
        PathBuf::from(repo_path),
        "echo".to_owned(),
    )
    .expect("runtime should initialize");

    app.send_message("/plan add minimax provider");
    let captured = wait_until(TEST_WAIT_TIMEOUT, || {
        app.tick();
        captured_prompt
            .lock()
            .expect("prompt mutex poisoned")
            .is_some()
    });
    assert!(captured, "planning prompt was not captured within timeout");

    let prompt = captured_prompt
        .lock()
        .expect("prompt mutex poisoned")
        .clone()
        .expect("provider should capture a prompt");

    assert!(prompt.contains("planning mode"));
    assert!(prompt.contains("short plain-text title"));
    assert!(prompt.contains("User request:"));
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

#[test]
fn picker_backspace_pops_filter_then_closes_overlay() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('h'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.picker.filter, "/h");

    app.handle_action(InputAction::Backspace);
    assert!(app.picker.is_open());
    assert_eq!(app.picker.filter, "/");

    app.handle_action(InputAction::Backspace);
    assert!(app.picker.is_open());
    assert!(app.picker.filter.is_empty());

    app.handle_action(InputAction::Backspace);
    assert!(!app.picker.is_open());
}

#[test]
fn picker_theme_selection_opens_subpicker_and_applies_theme() {
    use crate::chat::picker::PickerContext;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));
    for ch in "theme".chars() {
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        )));
    }
    app.handle_action(InputAction::Submit);
    assert_eq!(app.picker.context, PickerContext::ThemeSelect);
    assert!(app.picker.is_open());

    let selected_theme = app
        .picker
        .select_current()
        .expect("theme picker should have a selection");
    app.handle_action(InputAction::Submit);

    assert_eq!(app.theme_name(), selected_theme);
    assert_eq!(app.picker.context, PickerContext::Commands);
}

#[test]
fn demo_trigger_message_resets_timeline_and_starts_playback_queue() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.send_message("1");
    assert!(app.timeline.is_empty());
    assert!(app.demo_active());

    app.tick();
    assert_eq!(app.timeline.len(), 1);
    assert!(!app.timeline[0].body.is_empty());
}

#[test]
fn turn_helpers_ignore_blank_input_and_validate_plan_title() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    let initial_len = app.timeline.len();
    app.send_message("   ");
    assert_eq!(app.timeline.len(), initial_len);

    assert!(super::turns::extract_plan_title_and_body("").is_none());
    assert!(super::turns::extract_plan_title_and_body("###\n\nbody").is_none());
    let parsed = super::turns::extract_plan_title_and_body("Title\n\n1. step").expect("valid plan");
    assert_eq!(parsed.0, "Title");
    assert_eq!(parsed.1, "1. step");
}

#[test]
fn turn_failed_events_use_error_kind_for_cancel_hint() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.apply_agent_event_for_test(AgentEvent::TurnFailed {
        kind: crate::agent::runtime::RuntimeErrorKind::Cancelled,
        error: "not-the-cancelled-literal".into(),
    });
    assert!(matches!(
        app.timeline.last().map(|entry| &entry.kind),
        Some(EntryKind::Hint)
    ));

    app.apply_agent_event_for_test(AgentEvent::TurnFailed {
        kind: crate::agent::runtime::RuntimeErrorKind::ProviderFatal,
        error: "cancelled".into(),
    });
    let last = app.timeline.last().expect("warning entry should exist");
    assert!(matches!(last.kind, EntryKind::Warning));
    assert!(last.body.contains("Agent error: cancelled"));
}

#[test]
fn turn_failed_logs_redact_sensitive_error_text_for_app_log_path() {
    let (_, captured) = crate::observability::logging::with_captured_structured_events_for_test(|| {
        let mut app = new_test_app();
        let sensitive_error = "invalid API key sk-test-secret";

        app.apply_agent_event_for_test(AgentEvent::TurnStarted {
            turn_id: "turn-sensitive".into(),
        });
        app.apply_agent_event_for_test(AgentEvent::TurnFailed {
            kind: crate::agent::runtime::RuntimeErrorKind::ProviderFatal,
            error: sensitive_error.into(),
        });
    });

    let logged = captured
        .into_iter()
        .filter_map(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .find(|event| {
            event
                .get("event")
                .and_then(serde_json::Value::as_str)
                == Some("turn_failed")
                && event
                    .get("turn_id")
                    .and_then(serde_json::Value::as_str)
                    == Some("turn-sensitive")
                && event
                    .get("error_kind")
                    .and_then(serde_json::Value::as_str)
                    == Some("ProviderFatal")
        })
        .expect("expected redacted structured turn_failed app log event");

    let message = logged
        .get("message")
        .and_then(serde_json::Value::as_str)
        .expect("structured message should be present");
    assert!(message.contains("details=redacted"));
    assert!(message.contains("kind=ProviderFatal"));
    assert!(message.contains("turn_id=turn-sensitive"));
    assert!(!message.contains("invalid API key"));
    assert!(!message.contains("sk-test-secret"));
    assert_eq!(
        message,
        super::events::turn_failed_app_log_message(
            crate::agent::runtime::RuntimeErrorKind::ProviderFatal,
            Some("turn-sensitive"),
        )
    );
}

#[test]
fn quit_and_scroll_actions_follow_runtime_state_rules() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline_content_height.set(30);
    app.timeline_visible_height.set(10);
    app.scroll_offset = u16::MAX;

    app.handle_action(InputAction::ScrollUp);
    assert_eq!(app.scroll_offset, 17);
    app.handle_action(InputAction::ScrollDown);
    assert_eq!(app.scroll_offset, 20);

    app.handle_action(InputAction::Quit);
    assert!(app.should_quit());
}

#[test]
fn set_model_sends_command_and_updates_timeline() {
    let mut app = new_test_app();
    app.set_model("Qwen/Qwen3-8B");

    let hint = app.timeline.last().expect("hint entry should exist");
    assert!(matches!(hint.kind, EntryKind::Hint));
    assert!(hint.body.contains("Model switched to"));
    assert_eq!(app.model_name(), "Qwen/Qwen3-8B");
}

#[test]
fn picker_plan_command_prefills_composer_shortcut() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));
    for ch in "plan".chars() {
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        )));
    }

    app.handle_action(InputAction::Submit);
    assert_eq!(app.composer_text(), "/plan ");
}

#[test]
fn slash_when_composer_not_empty_does_not_open_picker() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.set_composer_text("x");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    assert!(!app.picker.is_open());
    assert!(app.composer_text().contains('/'));
}

#[test]
fn status_label_transitions_across_streaming_failure_and_recovery() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    assert_eq!(app.status_label(), "ready");

    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "t-1".into(),
    });
    assert_eq!(app.status_label(), "streaming…");

    app.active_turn_kind = Some(TurnKind::Plan);
    assert_eq!(app.status_label(), "thinking");

    app.apply_agent_event_for_test(AgentEvent::TurnFailed {
        kind: crate::agent::runtime::RuntimeErrorKind::ProviderTransient,
        error: "network".into(),
    });
    assert!(app.status_label().contains("error: network"));

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);
    assert_eq!(app.status_label(), "ready");
}

#[test]
fn submit_composer_sends_message_and_clears_input() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.set_composer_text("hello from composer");
    app.submit_composer();

    assert_eq!(app.composer_text(), "");
    assert!(
        app.timeline
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body.contains("hello from composer"))
    );
}

#[test]
fn discover_agents_command_adds_hint_without_starting_turn() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.send_message("/discover-agents");

    assert!(
        app.timeline
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == "/discover-agents")
    );
    let hint = app.timeline.last().expect("discover hint should exist");
    assert!(matches!(hint.kind, EntryKind::Hint));
    assert!(
        hint.body.contains("Discovering ACP agents"),
        "expected immediate discover-agents hint, got {:?}",
        hint.body
    );
    assert!(
        !app.is_streaming(),
        "discover-agents should not start a chat turn"
    );
}

#[test]
fn picker_discover_agents_command_adds_hint() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));
    for ch in "discover-agents".chars() {
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        )));
    }

    app.handle_action(InputAction::Submit);

    let hint = app.timeline.last().expect("discover hint should exist");
    assert!(matches!(hint.kind, EntryKind::Hint));
    assert!(hint.body.contains("Discovering ACP agents"));
}

#[test]
fn picker_model_command_opens_model_subpicker_context() {
    use crate::chat::picker::PickerContext;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));
    for ch in "model".chars() {
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        )));
    }
    app.handle_action(InputAction::Submit);

    assert_eq!(app.picker.context, PickerContext::ModelSelect);
    assert!(app.picker.is_open());
    assert_eq!(app.picker.title, "Select Model");
}

#[test]
fn quit_during_streaming_requests_cancel_without_exiting() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "streaming-turn".into(),
    });
    assert!(app.is_streaming());

    app.handle_action(InputAction::Quit);
    assert!(!app.should_quit());
}

#[test]
fn set_model_with_valid_config_switches_runtime_and_adds_hint() {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&base).expect("target dir should exist");
    let unique = format!(
        "chat-app-model-success-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos()
    );
    let repo_path = base.join(unique);
    std::fs::create_dir_all(repo_path.join("config")).expect("create config dir");
    std::fs::write(
        repo_path.join("config/provider.json"),
        r#"{
  "api_key": "test-key",
  "base_url": "https://example.com/v1",
  "model": "Qwen/Qwen3-8B"
}"#,
    )
    .expect("write provider config");

    let repo_str = repo_path.to_string_lossy().to_string();
    let mut app = ChatApp::new_for_test(&repo_str).expect("test app should initialize");
    app.set_model("Qwen/Qwen3-32B");

    assert_eq!(app.model_name(), "Qwen/Qwen3-32B");
    let hint = app.timeline.last().expect("hint entry should exist");
    assert!(matches!(hint.kind, EntryKind::Hint));
    assert!(hint.body.contains("Model switched"));

    std::fs::remove_dir_all(&repo_path).expect("cleanup scratch repo");
}

#[test]
fn private_text_helpers_handle_unicode_and_home_paths() {
    let home = std::env::var("HOME").expect("HOME should exist");
    let inside_home = format!("{home}/projects/blazar");
    assert_eq!(shorten_home(&inside_home), "~/projects/blazar");
    assert_eq!(shorten_home("/opt/work"), "/opt/work");

    assert_eq!(preview_text("你好世界hello", 2), "你好");
    assert_eq!(
        summarize_tool_arguments(&"x".repeat(70)).chars().count(),
        60
    );
    assert_eq!(summarize_tool_output("first\nsecond"), "first");
}

#[test]
fn picker_submit_regular_command_and_navigation_actions_work() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    let selected_before = app.picker.selected_index();
    app.handle_action(InputAction::ScrollDown);
    app.handle_action(InputAction::PickerDown);
    app.handle_action(InputAction::ScrollUp);
    app.handle_action(InputAction::PickerUp);
    assert_eq!(app.picker.selected_index(), selected_before);

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    assert!(app.picker.filter.is_empty());

    for ch in "help".chars() {
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        )));
    }
    app.handle_action(InputAction::Submit);
    assert!(
        app.timeline
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == "/help")
    );
}

#[test]
fn picker_model_selection_trims_checkmark_suffix_before_switch() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&base).expect("target dir should exist");
    let unique = format!(
        "chat-app-model-trim-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos()
    );
    let repo_path = base.join(unique);
    std::fs::create_dir_all(repo_path.join("config")).expect("create config dir");
    std::fs::write(
        repo_path.join("config/provider.json"),
        r#"{
  "api_key": "test-key",
  "base_url": "https://example.com/v1",
  "model": "Qwen/Qwen3-8B"
}"#,
    )
    .expect("write provider config");

    let repo_str = repo_path.to_string_lossy().to_string();
    let mut app = ChatApp::new_for_test(&repo_str).expect("test app should initialize");
    app.model_name = "Qwen/Qwen3-8B".into();
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));
    for ch in "model".chars() {
        app.handle_action(InputAction::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        )));
    }
    app.handle_action(InputAction::Submit); // open model picker
    app.handle_action(InputAction::Submit); // choose first model item (contains ✓ suffix)

    assert_eq!(app.model_name(), "Qwen/Qwen3-8B");
    let hint = app.timeline.last().expect("hint entry should exist");
    assert!(!hint.body.contains('✓'));

    std::fs::remove_dir_all(&repo_path).expect("cleanup scratch repo");
}

#[test]
fn input_backspace_and_toggle_details_actions_update_local_state() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('b'),
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::Backspace);
    assert_eq!(app.composer_text(), "a");

    assert!(!app.show_details());
    app.handle_action(InputAction::ToggleDetails);
    assert!(app.show_details());
}

#[test]
fn event_handlers_append_to_existing_thinking_and_response_entries() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline.clear();
    app.timeline.push(TimelineEntry::thinking("seed"));

    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: " +more".into(),
    });
    assert_eq!(app.timeline.len(), 1);
    assert!(app.timeline[0].body.contains("seed +more"));
    assert!(app.timeline[0].details.contains(" +more"));

    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: "answer".into(),
    });
    assert_eq!(app.timeline.len(), 2);
    app.apply_agent_event_for_test(AgentEvent::TextDelta {
        text: " tail".into(),
    });
    assert!(app.timeline[1].body.ends_with(" tail"));
}

#[test]
fn tool_call_completed_without_matching_start_is_ignored_safely() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    let initial_len = app.timeline.len();
    app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
        call_id: "missing".into(),
        output: "ignored".into(),
        is_error: false,
    });
    assert_eq!(app.timeline.len(), initial_len);
}

#[test]
fn tool_events_log_include_turn_and_tool_name() {
    let (_, captured) = crate::observability::logging::with_captured_structured_events_for_test(|| {
        let mut app = new_test_app();

        app.apply_agent_event_for_test(AgentEvent::TurnStarted {
            turn_id: "turn-tool-observe".into(),
        });
        app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
            call_id: "call-log".into(),
            tool_name: "bash".into(),
            kind: crate::agent::tools::ToolKind::Local,
            arguments: r#"{"command":"echo hi"}"#.into(),
            batch_id: 0,
            replay_index: 0,
            normalized_claims: Vec::new(),
        });
    });

    let tool_started = captured
        .into_iter()
        .filter_map(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .find(|event| {
            event
                .get("event")
                .and_then(serde_json::Value::as_str)
                == Some("tool_call_started")
                && event
                    .get("turn_id")
                    .and_then(serde_json::Value::as_str)
                    == Some("turn-tool-observe")
                && event
                    .get("tool_name")
                    .and_then(serde_json::Value::as_str)
                    == Some("bash")
        })
        .expect("expected structured tool_call_started event with turn_id+tool_name");

    assert!(
        tool_started
            .get("message")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|message| message.contains("tool=bash")),
        "tool event log message should include tool identifier for auditability"
    );
    assert!(
        tool_started
            .get("turn_id")
            .and_then(serde_json::Value::as_str)
            == Some("turn-tool-observe"),
        "tool event logs should include turn_id"
    );
    assert!(
        tool_started
            .get("tool_name")
            .and_then(serde_json::Value::as_str)
            == Some("bash"),
        "tool event logs should include turn_id and tool_name fields"
    );
}

#[test]
fn tool_call_completed_logs_include_turn_and_resolved_tool_name() {
    let (_, captured) = crate::observability::logging::with_captured_structured_events_for_test(|| {
        let mut app = new_test_app();

        app.apply_agent_event_for_test(AgentEvent::TurnStarted {
            turn_id: "turn-tool-complete".into(),
        });
        app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
            call_id: "call-complete-log".into(),
            tool_name: "bash".into(),
            kind: crate::agent::tools::ToolKind::Local,
            arguments: r#"{"command":"echo hi"}"#.into(),
            batch_id: 0,
            replay_index: 0,
            normalized_claims: Vec::new(),
        });
        app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
            call_id: "call-complete-log".into(),
            output: "ok".into(),
            is_error: false,
        });
    });

    let tool_completed = captured
        .into_iter()
        .filter_map(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .find(|event| {
            event
                .get("event")
                .and_then(serde_json::Value::as_str)
                == Some("tool_call_completed")
                && event
                    .get("turn_id")
                    .and_then(serde_json::Value::as_str)
                    == Some("turn-tool-complete")
                && event
                    .get("tool_name")
                    .and_then(serde_json::Value::as_str)
                    == Some("bash")
        })
        .expect("expected structured tool_call_completed event with turn_id+tool_name");

    assert_eq!(
        tool_completed
            .get("turn_id")
            .and_then(serde_json::Value::as_str),
        Some("turn-tool-complete"),
    );
    assert_eq!(
        tool_completed
            .get("tool_name")
            .and_then(serde_json::Value::as_str),
        Some("bash"),
    );
}

#[test]
fn picker_commands_context_supports_non_slash_labels() {
    use crate::chat::picker::{ModalPicker, PickerContext, PickerItem};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.picker = ModalPicker::with_context(
        "Commands",
        vec![PickerItem::new("custom command", "desc")],
        PickerContext::Commands,
    );
    app.picker.open();

    app.handle_action(InputAction::Submit);
    assert!(
        app.timeline
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == "custom command")
    );
}

#[test]
fn tool_call_completed_sets_error_status_and_summary() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-err".into(),
        tool_name: "bash".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"command":"exit 1"}"#.into(),
        batch_id: 0,
        replay_index: 0,
        normalized_claims: Vec::new(),
    });
    app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
        call_id: "call-err".into(),
        output: "first line\nsecond line".into(),
        is_error: true,
    });

    let entry = app.timeline.last().expect("tool entry should exist");
    assert!(matches!(
        entry.kind,
        EntryKind::ToolCall {
            status: ToolCallStatus::Error,
            ..
        }
    ));
    assert_eq!(entry.body, "first line");
    assert!(entry.details.contains("second line"));
}

#[test]
fn agent_tool_calls_render_acp_badge() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-acp".into(),
        tool_name: "configured_reviewer".into(),
        kind: crate::agent::tools::ToolKind::Agent { is_acp: true },
        arguments: r#"{"prompt":"review the patch"}"#.into(),
        batch_id: 0,
        replay_index: 0,
        normalized_claims: Vec::new(),
    });

    let entry = app.timeline.last().expect("tool entry should exist");
    assert!(matches!(
        entry.kind,
        EntryKind::ToolCall {
            kind: crate::agent::tools::ToolKind::Agent { is_acp: true },
            ..
        }
    ));

    let rendered = render_to_lines_for_test(&mut app, 100, 35).join("\n");
    assert!(rendered.contains("configured_reviewer"));
    assert!(rendered.contains("(ACP)"));
}

#[test]
fn chat_timeline_preserves_capability_metadata_for_tool_entries() {
    let mut app = new_test_app();

    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-meta".into(),
        tool_name: "write_file".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"path":"src/lib.rs"}"#.into(),
        batch_id: 7,
        replay_index: 2,
        normalized_claims: vec![
            "fs:src/lib.rs#read-write".into(),
            "process:bash#exclusive".into(),
        ],
    });

    let entry = app.timeline.last().expect("tool entry should exist");
    assert!(entry.details.contains(r#"{"path":"src/lib.rs"}"#));
    assert!(entry.details.contains("batch_id=7"));
    assert!(entry.details.contains("replay_index=2"));
    assert!(
        entry
            .details
            .contains("normalized_claims=fs:src/lib.rs#read-write,process:bash#exclusive")
    );
}

#[test]
fn chat_timeline_renders_sorted_claims_for_stable_metadata() {
    let mut app = new_test_app();

    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-meta-unstable".into(),
        tool_name: "write_file".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"path":"src/lib.rs"}"#.into(),
        batch_id: 8,
        replay_index: 1,
        normalized_claims: vec![
            "process:bash#exclusive".into(),
            "fs:src/lib.rs#read-write".into(),
        ],
    });

    let entry = app.timeline.last().expect("tool entry should exist");
    assert!(
        entry
            .details
            .contains("normalized_claims=fs:src/lib.rs#read-write,process:bash#exclusive")
    );
}

#[test]
fn turn_complete_plan_with_non_extractable_body_leaves_entry_untitled() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.timeline
        .push(TimelineEntry::response("###").with_title("Existing"));
    app.active_turn_kind = Some(TurnKind::Plan);
    app.active_turn_title = Some("Temp title".into());

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);
    let last = app.timeline.last().expect("assistant entry should exist");
    assert_eq!(last.title.as_deref(), Some("Existing"));
    assert_eq!(last.body, "###");
    assert!(app.active_turn_kind.is_none());
    assert!(app.active_turn_title.is_none());
}

#[test]
fn build_pending_turn_and_dispatch_helpers_cover_plan_and_chat_paths() {
    let chat = super::turns::build_pending_turn("  hello ");
    assert_eq!(chat.user_text, "hello");
    assert_eq!(chat.runtime_prompt, "hello");
    assert_eq!(chat.kind, TurnKind::Chat);

    let plan = super::turns::build_pending_turn("/plan   improve tests");
    assert_eq!(plan.kind, TurnKind::Plan);
    assert!(plan.runtime_prompt.contains("planning mode"));
    assert!(plan.runtime_prompt.contains("User request:\nimprove tests"));

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");
    app.dispatch_next_queued();
    assert!(app.pending_messages.is_empty());
}
