use blazar::chat::app::ChatApp;
use blazar::chat::event_loop::resolve_repo_path;
use blazar::chat::input::InputAction;
use blazar::chat::model::Actor;
use blazar::chat::picker::PickerContext;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn create_unique_test_workspace(test_name: &str) -> PathBuf {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-workspaces");
    std::fs::create_dir_all(&base).expect("create test-workspaces dir");

    let unique = format!(
        "{}-{}",
        test_name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos()
    );
    let workspace = base.join(unique);
    std::fs::create_dir_all(&workspace).expect("create unique workspace");

    let _ = std::process::Command::new("git")
        .args(["init"])
        .current_dir(&workspace)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&workspace)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&workspace)
        .output();

    workspace
}

fn extract_continue_plan_id(composer_text: &str) -> &str {
    composer_text
        .strip_prefix("/plan --continue ")
        .expect("composer should contain continue command")
}

fn read_plan_json(workspace: &Path, plan_id: &str) -> serde_json::Value {
    let plan_path = workspace
        .join(".blazar")
        .join("plans")
        .join(format!("{plan_id}.json"));
    let raw = std::fs::read_to_string(plan_path).expect("plan session json should exist");
    serde_json::from_str(&raw).expect("plan json should parse")
}

fn assert_submits_as_plain_message(app: &mut ChatApp, text: &str) {
    app.set_composer_text(text);
    app.handle_action(InputAction::Submit);
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == text),
        "expected malformed form {text:?} to be submitted as a regular message"
    );
}

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
                && entry.title.is_some()
                && !entry.body.trim().is_empty()
        }) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    panic!("expected /plan submit to run planning prompt flow");
}

#[test]
fn chat_runtime_submit_plan_goal_from_composer_dispatches_plan_command() {
    let workspace = create_unique_test_workspace("composer_plan_goal");
    let mut app = ChatApp::new_for_test(workspace.to_str().unwrap()).expect("test app");
    app.set_composer_text("/plan ship command arg parsing");

    app.handle_action(InputAction::Submit);

    let composer = app.composer_text();
    let plan_id = extract_continue_plan_id(&composer);
    assert!(
        !plan_id.is_empty(),
        "plan command should leave continue command in composer"
    );
    let saved = read_plan_json(&workspace, plan_id);
    assert_eq!(
        saved.get("goal").and_then(serde_json::Value::as_str),
        Some("ship command arg parsing")
    );

    let _ = std::fs::remove_dir_all(&workspace);
}

#[test]
fn chat_runtime_submit_plan_continue_from_composer_dispatches_plan_command() {
    let workspace = create_unique_test_workspace("composer_plan_continue");
    let mut app = ChatApp::new_for_test(workspace.to_str().unwrap()).expect("test app");
    app.set_composer_text("/plan bootstrap command flow");
    app.handle_action(InputAction::Submit);
    let seed_plan_id = extract_continue_plan_id(&app.composer_text()).to_owned();

    app.set_composer_text(&format!(
        "/plan --continue {seed_plan_id} finish composer dispatch"
    ));
    app.handle_action(InputAction::Submit);

    let composer = app.composer_text();
    let resumed_plan_id = extract_continue_plan_id(&composer);
    assert_eq!(
        resumed_plan_id, seed_plan_id,
        "continuation should keep using the same plan id"
    );
    let saved = read_plan_json(&workspace, resumed_plan_id);
    assert_eq!(
        saved.get("goal").and_then(serde_json::Value::as_str),
        Some("finish composer dispatch")
    );

    let _ = std::fs::remove_dir_all(&workspace);
}

#[test]
fn chat_runtime_does_not_treat_planner_as_plan_command() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    assert_submits_as_plain_message(&mut app, "/planner");
}

#[test]
fn chat_runtime_does_not_treat_planx_as_plan_command() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    assert_submits_as_plain_message(&mut app, "/planx");
}

#[test]
fn chat_runtime_rejects_malformed_plan_continue_flag_form() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    assert_submits_as_plain_message(&mut app, "/plan --continue123 bad");
}
