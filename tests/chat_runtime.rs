use blazar::chat::app::ChatApp;
use blazar::chat::input::InputAction;
use blazar::chat::workspace::{WorkspaceApp, WorkspaceFocus, WorkspaceView};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn enter_key_submits_composer_content_and_clears() {
    let mut app = ChatApp::new_for_test(REPO_ROOT);
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
    let _app = ChatApp::new_for_test(REPO_ROOT);
    let action = InputAction::from_key_event(KeyEvent::from(KeyCode::Esc));

    assert!(matches!(action, InputAction::Quit));
}

#[test]
fn ctrl_c_requests_quit() {
    let _app = ChatApp::new_for_test(REPO_ROOT);
    let action =
        InputAction::from_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

    assert!(matches!(action, InputAction::Quit));
}

#[test]
fn digit_shortcuts_switch_workspace_views() {
    let chat = InputAction::from_key_event(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
    let git = InputAction::from_key_event(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
    let sessions =
        InputAction::from_key_event(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));

    assert!(matches!(chat, InputAction::SelectChatView));
    assert!(matches!(git, InputAction::SelectGitView));
    assert!(matches!(sessions, InputAction::SelectSessionsView));
}

#[test]
fn character_input_is_forwarded_to_composer() {
    let mut app = ChatApp::new_for_test(REPO_ROOT);

    let action = InputAction::from_key_event(KeyEvent::from(KeyCode::Char('a')));
    app.handle_action(action);

    // Composer should have received the character
    assert!(app.composer_text().contains('a'));
}

#[test]
fn tab_shortcut_cycles_workspace_focus() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.focus(), WorkspaceFocus::Content);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.focus(), WorkspaceFocus::Footer);
}

#[test]
fn workspace_routes_shortcuts_and_forwards_other_keys() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('2'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.active_view(), WorkspaceView::Git);
    assert_eq!(app.focus(), WorkspaceFocus::Content);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('3'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.active_view(), WorkspaceView::Sessions);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
    )));
    assert!(app.chat().composer_text().contains('a'));
}

#[test]
fn digit_shortcuts_type_into_composer_when_footer_is_focused() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));
    assert_eq!(app.focus(), WorkspaceFocus::Footer);

    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('1'),
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('2'),
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::from_key_event(KeyEvent::new(
        KeyCode::Char('3'),
        KeyModifiers::NONE,
    )));

    assert_eq!(app.active_view(), WorkspaceView::Chat);
    assert_eq!(app.chat().composer_text(), "123");
}

#[test]
fn app_tracks_quit_flag() {
    let mut app = ChatApp::new_for_test(REPO_ROOT);
    assert!(!app.should_quit());

    app.handle_action(InputAction::Quit);
    assert!(app.should_quit());
}

// Task 6: workspace runtime wiring — quit and shortcut behavior
#[test]
fn workspace_app_quits_on_quit_action() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);
    assert!(!app.should_quit());

    app.handle_action(InputAction::Quit);
    assert!(
        app.should_quit(),
        "WorkspaceApp must expose should_quit() after Quit action"
    );
}

#[test]
fn workspace_default_view_is_chat() {
    let app = WorkspaceApp::new_for_test(REPO_ROOT);
    assert_eq!(
        app.active_view(),
        WorkspaceView::Chat,
        "Chat must be the default home view"
    );
}
