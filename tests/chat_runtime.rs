use blazar::chat::app::ChatApp;
use blazar::chat::input::InputAction;
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
fn character_input_is_forwarded_to_composer() {
    let mut app = ChatApp::new_for_test(REPO_ROOT);

    let action = InputAction::from_key_event(KeyEvent::from(KeyCode::Char('a')));
    app.handle_action(action);

    // Composer should have received the character
    assert!(app.composer_text().contains('a'));
}

#[test]
fn app_tracks_quit_flag() {
    let mut app = ChatApp::new_for_test(REPO_ROOT);
    assert!(!app.should_quit());

    app.handle_action(InputAction::Quit);
    assert!(app.should_quit());
}
