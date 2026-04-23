use blazar::chat::app::ChatApp;
use blazar::chat::event_loop::resolve_repo_path;
use blazar::chat::input::InputAction;
use blazar::chat::model::Actor;
use blazar::chat::picker::PickerContext;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn enter_key_submits_composer_content_and_clears() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.set_composer_text("Hello Spirit");

    let action = InputAction::from_key_event(KeyEvent::from(KeyCode::Enter));
    app.handle_action(action);

    assert!(
        app.messages()
            .iter()
            .any(|msg| msg.body.contains("Hello Spirit"))
    );
    assert_eq!(app.composer_text(), "");
}

#[test]
fn esc_key_requests_quit() {
    let _app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let action = InputAction::from_key_event(KeyEvent::from(KeyCode::Esc));

    assert!(matches!(action, InputAction::Quit));
}

#[test]
fn ctrl_c_requests_quit() {
    let _app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let action =
        InputAction::from_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

    assert!(matches!(action, InputAction::Quit));
}

#[test]
fn character_input_is_forwarded_to_composer() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

    let action = InputAction::from_key_event(KeyEvent::from(KeyCode::Char('a')));
    app.handle_action(action);

    assert!(app.composer_text().contains('a'));
}

#[test]
fn digit_keys_are_regular_input_again() {
    let digit = InputAction::from_key_event(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));

    assert!(
        matches!(
            digit,
            InputAction::Key(KeyEvent {
                code: KeyCode::Char('2'),
                ..
            })
        ),
        "digit shortcuts should no longer be reserved for view switching"
    );
}

#[test]
fn app_tracks_quit_flag() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    assert!(!app.should_quit());

    app.handle_action(InputAction::Quit);
    assert!(app.should_quit());
}

#[test]
fn resolve_repo_path_uses_schema_repopath_default() {
    let schema = serde_json::json!({
        "properties": {
            "workspace": {
                "properties": {
                    "repoPath": { "default": "/home/user/myproject" }
                }
            }
        }
    });
    assert_eq!(resolve_repo_path(&schema), "/home/user/myproject");
}

#[test]
fn resolve_repo_path_falls_back_to_non_empty_string_when_schema_empty() {
    let schema = serde_json::json!({});
    let path = resolve_repo_path(&schema);
    assert!(
        !path.is_empty(),
        "fallback must not be empty; got: {path:?}"
    );
}

#[test]
fn chat_runtime_picker_theme_command_opens_theme_subpicker() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

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
}

#[test]
fn chat_runtime_picker_model_command_opens_model_subpicker() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

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
}

#[test]
fn chat_runtime_discover_agents_stays_local_without_streaming() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

    app.send_message("/discover-agents");

    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == "/discover-agents")
    );
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("Discovering ACP agents"))
    );
    assert!(!app.is_streaming());
}

#[test]
fn chat_runtime_submit_exact_plan_from_composer_uses_planning_turn() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.set_composer_text("/plan");

    app.handle_action(InputAction::Submit);

    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == "/plan"),
        "submitting /plan should create a user turn entry"
    );

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        app.tick();
        if app.timeline().iter().any(|entry| {
            entry.actor == Actor::Assistant
                && (entry.title.as_deref() == Some("You are in planning mode.")
                    || entry
                        .body
                        .contains("Generate a concise implementation plan only."))
        }) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    panic!("expected /plan submit to run planning prompt flow");
}
