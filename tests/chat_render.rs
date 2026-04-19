use blazar::chat::app::ChatApp;
use blazar::chat::view::{render_frame, render_to_lines_for_test};
use ratatui_core::{backend::TestBackend, style::Color, terminal::Terminal};

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn chat_view_renders_title_bar_and_timeline() {
    let app = ChatApp::new_for_test(REPO_ROOT);
    let lines = render_to_lines_for_test(&app, 100, 30);

    assert!(
        lines.iter().any(|line| line.contains("blazar")),
        "title bar should contain 'blazar'"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Tell me what you'd like to explore")),
        "timeline should show initial greeting"
    );
}

#[test]
fn chat_view_renders_status_bar() {
    let app = ChatApp::new_for_test(REPO_ROOT);
    let lines = render_to_lines_for_test(&app, 100, 30);

    assert!(
        lines.iter().any(|line| line.contains("ready")),
        "status bar should show 'ready'"
    );
}

#[test]
fn timeline_does_not_emit_raw_ansi_escape_sequences() {
    let app = ChatApp::new_for_test(REPO_ROOT);
    let lines = render_to_lines_for_test(&app, 100, 30);

    assert!(
        lines.iter().all(|line| !line.contains('\u{1b}')),
        "timeline should render styled text, not raw ANSI sequences"
    );
}

#[test]
fn timeline_entries_have_identity_markers() {
    let app = ChatApp::new_for_test(REPO_ROOT);
    let lines = render_to_lines_for_test(&app, 100, 30);

    assert!(
        lines.iter().any(|line| line.contains('●')),
        "timeline entries should have ● identity markers"
    );
}

#[test]
fn title_bar_uses_terminal_default_background() {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let app = ChatApp::new_for_test(REPO_ROOT);

    terminal
        .draw(|frame| render_frame(frame, &app, 1_200))
        .expect("chat frame should render");

    // Title bar is row 0 — background should be the terminal default (no override)
    let first_row_cell = &terminal.backend().buffer().content()[50]; // middle of title
    assert_eq!(
        first_row_cell.bg,
        Color::Reset,
        "title bar should use the terminal default background"
    );
}

/// Simulates the interactive flow: start → type "1" → submit → verify echo response.
#[test]
fn interactive_send_message_shows_echo_response() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = ChatApp::new_for_test(REPO_ROOT);

    // Step 1: initial state — only greeting visible
    let lines_before = render_to_lines_for_test(&app, 80, 35);
    assert!(
        lines_before
            .iter()
            .any(|l| l.contains("Tell me what you'd like to explore")),
        "initial state should show greeting"
    );
    assert!(
        !lines_before.iter().any(|l| l.contains("I hear you")),
        "no echo response before user input"
    );

    // Step 2: simulate typing "1"
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('1'),
        KeyModifiers::NONE,
    )));
    let lines_typing = render_to_lines_for_test(&app, 80, 35);
    assert!(
        lines_typing.iter().any(|l| l.contains('1')),
        "composer should show typed character"
    );

    // Step 3: press Enter to submit
    app.handle_action(InputAction::Submit);

    // Step 4: verify the echo response appeared in the rendered output
    let lines_after = render_to_lines_for_test(&app, 80, 35);
    assert!(
        lines_after.iter().any(|l| l.contains("I hear you")),
        "echo response should appear after submit"
    );
    assert!(
        lines_after.iter().any(|l| l.contains('1')),
        "user message '1' should appear in timeline"
    );
}
