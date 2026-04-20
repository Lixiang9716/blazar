use blazar::chat::app::ChatApp;
use blazar::chat::view::{render_frame, render_to_lines_for_test};
use ratatui_core::{backend::TestBackend, style::Color, terminal::Terminal};

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn chat_view_renders_title_bar_and_timeline() {
    let mut app = ChatApp::new_for_test(REPO_ROOT);
    let lines = render_to_lines_for_test(&mut app, 100, 35);

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
    let mut app = ChatApp::new_for_test(REPO_ROOT);
    let lines = render_to_lines_for_test(&mut app, 100, 35);

    assert!(
        lines.iter().any(|line| line.contains("commands")),
        "status bar should show '/ commands'"
    );
}

#[test]
fn slash_opens_command_picker_overlay() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = ChatApp::new_for_test(REPO_ROOT);

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    let lines = render_to_lines_for_test(&mut app, 100, 35);

    assert!(
        lines.iter().any(|line| line.contains("Commands")),
        "picker overlay should show its title"
    );
    assert!(
        lines.iter().any(|line| line.contains("/help")),
        "picker overlay should show command entries"
    );
}

#[test]
fn picker_navigation_reaches_later_commands() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = ChatApp::new_for_test(REPO_ROOT);

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    for _ in 0..12 {
        app.handle_action(InputAction::PickerDown);
    }

    let lines = render_to_lines_for_test(&mut app, 100, 35);

    assert!(
        lines
            .iter()
            .any(|line| line.contains("/tools") || line.contains("/agents")),
        "picker navigation should reach later command entries"
    );
}

#[test]
fn closing_picker_routes_typing_back_to_composer() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::time::Duration;

    let mut app = ChatApp::new_for_test(REPO_ROOT);
    let animated_overlay = app
        .picker
        .overlay_state()
        .clone()
        .with_duration(Duration::from_millis(250));
    *app.picker.overlay_state_mut() = animated_overlay;

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::Quit);
    assert!(
        app.picker.is_visible(),
        "picker should still render while closing animation runs"
    );

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
    )));
    assert_eq!(
        app.composer_text(),
        "x",
        "input should route to composer while picker is closing"
    );
}

#[test]
fn timeline_does_not_emit_raw_ansi_escape_sequences() {
    let mut app = ChatApp::new_for_test(REPO_ROOT);
    let lines = render_to_lines_for_test(&mut app, 100, 35);

    assert!(
        lines.iter().all(|line| !line.contains('\u{1b}')),
        "timeline should render styled text, not raw ANSI sequences"
    );
}

#[test]
fn timeline_entries_have_identity_markers() {
    let mut app = ChatApp::new_for_test(REPO_ROOT);
    let lines = render_to_lines_for_test(&mut app, 100, 35);

    assert!(
        lines.iter().any(|line| line.contains('●')),
        "timeline entries should have ● identity markers"
    );
}

#[test]
fn title_bar_uses_terminal_default_background() {
    let backend = TestBackend::new(100, 35);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = ChatApp::new_for_test(REPO_ROOT);

    terminal
        .draw(|frame| render_frame(frame, &mut app, 1_200))
        .expect("chat frame should render");

    // Title bar is row 0 — background should be the terminal default (no override)
    let first_row_cell = &terminal.backend().buffer().content()[50]; // middle of title
    assert_eq!(
        first_row_cell.bg,
        Color::Reset,
        "title bar should use the terminal default background"
    );
}

#[test]
fn picker_render_persists_overlay_layout_state() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let backend = TestBackend::new(100, 35);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = ChatApp::new_for_test(REPO_ROOT);
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    terminal
        .draw(|frame| render_frame(frame, &mut app, 1_200))
        .expect("chat frame should render");

    assert!(
        app.picker.overlay_state().inner_area().is_some(),
        "picker overlay layout should persist in picker state after render"
    );
}

/// Simulates the interactive flow: start → type "hi" → submit → verify echo response.
#[test]
fn interactive_send_message_shows_echo_response() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = ChatApp::new_for_test(REPO_ROOT);

    // Step 1: initial state — only greeting visible
    let lines_before = render_to_lines_for_test(&mut app, 80, 35);
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

    // Step 2: simulate typing "hi"
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('h'),
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('i'),
        KeyModifiers::NONE,
    )));
    let lines_typing = render_to_lines_for_test(&mut app, 80, 35);
    assert!(
        lines_typing.iter().any(|l| l.contains("hi")),
        "composer should show typed characters"
    );

    // Step 3: press Enter to submit
    app.handle_action(InputAction::Submit);

    // Agent response arrives asynchronously — give the background thread time.
    std::thread::sleep(std::time::Duration::from_millis(200));
    app.tick();

    // Step 4: verify the echo response appeared in the rendered output
    let lines_after = render_to_lines_for_test(&mut app, 80, 35);
    assert!(
        lines_after.iter().any(|l| l.contains("Echo:")),
        "echo response should appear after submit"
    );
    assert!(
        lines_after.iter().any(|l| l.contains("hi")),
        "user message 'hi' should appear in timeline"
    );
}
