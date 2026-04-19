use blazar::chat::app::ChatApp;
use blazar::chat::view::{render_frame, render_to_lines_for_test};
use ratatui_core::{backend::TestBackend, style::Color, terminal::Terminal};

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn chat_view_renders_spirit_pane_and_message_pane() {
    let app = ChatApp::new_for_test(REPO_ROOT);
    let lines = render_to_lines_for_test(&app, 100, 30);

    assert!(
        lines
            .iter()
            .any(|line| line.contains("Spirit / 星糖导航马"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Tell me what you'd like to explore"))
    );
}

#[test]
fn spirit_pane_shows_the_mascot_and_status_copy() {
    let app = ChatApp::new_for_test(REPO_ROOT);
    let lines = render_to_lines_for_test(&app, 100, 30);

    assert!(
        lines
            .iter()
            .any(|line| line.contains("Waiting with a sprinkle of stardust"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("▀") || line.contains("▄") || line.contains("█"))
    );
}

#[test]
fn spirit_pane_does_not_emit_raw_ansi_escape_sequences() {
    let app = ChatApp::new_for_test(REPO_ROOT);
    let lines = render_to_lines_for_test(&app, 100, 30);

    assert!(
        lines.iter().all(|line| !line.contains('\u{1b}')),
        "chat spirit pane should render mascot glyphs, not raw ANSI sequences"
    );
}

#[test]
fn spirit_pane_preserves_mascot_colors_in_terminal_buffer() {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let app = ChatApp::new_for_test(REPO_ROOT);

    terminal
        .draw(|frame| render_frame(frame, &app, 1_200))
        .expect("chat frame should render");

    let has_colored_mascot_cell = terminal.backend().buffer().content().iter().any(|cell| {
        matches!(cell.symbol(), "▀" | "▄" | "█")
            && (cell.fg != Color::Reset || cell.bg != Color::Reset)
    });

    assert!(
        has_colored_mascot_cell,
        "spirit pane should preserve mascot colors in the ratatui buffer"
    );
}
