use blazar::chat::app::ChatApp;
use blazar::chat::view::{render_frame, render_to_lines_for_test};
use ratatui_core::{backend::TestBackend, style::Color, terminal::Terminal};

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn chat_view_renders_title_bar_and_timeline() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let lines = render_to_lines_for_test(&mut app, 100, 35);

    assert!(
        lines
            .iter()
            .any(|line| line.to_lowercase().contains("blazar")),
        "title bar should contain 'Blazar'"
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
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let lines = render_to_lines_for_test(&mut app, 100, 35);

    assert!(
        lines.iter().any(|line| line.contains("commands")),
        "status bar should show '/ commands'"
    );
    assert!(
        lines.iter().any(|line| line.contains("refs:-")),
        "status bar should render refs:- fallback when no file references are known"
    );
}

#[test]
fn status_row_renders_path_branch_pr_and_references() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.set_pr_label_for_test(Some("PR#42 improve timeline".to_owned()));
    app.set_referenced_files_for_test(vec!["src/chat/view/mod.rs".to_owned()]);

    let lines = render_to_lines_for_test(&mut app, 130, 24);

    assert!(lines.iter().any(|line| line.contains("~/blazar")));
    assert!(lines.iter().any(|line| line.contains("main")));
    assert!(lines.iter().any(|line| line.contains("PR#42")));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("src/chat/view/mod.rs"))
    );
}

#[test]
fn chat_view_renders_status_input_mode_rows_in_users_region() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let lines = render_to_lines_for_test(&mut app, 100, 22);
    let users_rows = &lines[lines.len().saturating_sub(3)..];

    assert!(
        users_rows[1].contains("> "),
        "users input row should render the prompt"
    );
    assert!(
        users_rows[2].contains("AUTO"),
        "users mode row should render AUTO mode"
    );
    assert!(
        users_rows[2].contains("echo"),
        "users mode row should render the active model"
    );
}

#[test]
fn chat_view_keeps_mode_row_render_path_in_tight_heights() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let lines = render_to_lines_for_test(&mut app, 100, 3);

    assert!(
        lines.iter().any(|line| line.contains("AUTO")),
        "users mode row should still render in tight heights"
    );
}

#[test]
fn mode_row_renders_context_ratio_when_available() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.set_context_usage_for_test(1200, 8000);

    let lines = render_to_lines_for_test(&mut app, 120, 24);
    let users_rows = &lines[lines.len().saturating_sub(3)..];

    assert!(
        users_rows[2].contains("1200/8000 (15%)"),
        "mode row should render context ratio when available"
    );
}

#[test]
fn slash_renders_inline_command_matches_in_status_row() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('h'),
        KeyModifiers::NONE,
    )));
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('e'),
        KeyModifiers::NONE,
    )));

    let lines = render_to_lines_for_test(&mut app, 100, 22);

    assert!(
        lines.iter().any(|line| line.contains("/help")),
        "status row should render inline slash command matches"
    );
    assert!(
        lines.iter().all(|line| !line.contains("Commands")),
        "typing slash should not open command picker overlay"
    );
}

#[test]
fn slash_status_row_normalizes_multiline_query_text() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.set_composer_text("/help\nnext\r\nfinal");

    let lines = render_to_lines_for_test(&mut app, 120, 22);
    let users_rows = &lines[lines.len().saturating_sub(3)..];

    assert!(
        users_rows[0].contains("/help next final"),
        "status row should normalize slash query CR/LF and collapse repeated whitespace"
    );
}

#[test]
fn slash_status_row_and_matcher_share_normalization_for_crlf_queries() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.set_composer_text("/help\r\n   ");

    let lines = render_to_lines_for_test(&mut app, 120, 22);
    let users_rows = &lines[lines.len().saturating_sub(3)..];

    assert!(
        users_rows[0].contains("/help · /help"),
        "status row and matcher should use the same normalized slash query"
    );
}

#[test]
fn picker_navigation_reaches_later_commands() {
    use blazar::chat::input::InputAction;

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.picker.open();

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

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let animated_overlay = app
        .picker
        .overlay_state()
        .clone()
        .with_duration(Duration::from_millis(250));
    *app.picker.overlay_state_mut() = animated_overlay;

    app.picker.open();
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
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let lines = render_to_lines_for_test(&mut app, 100, 35);

    assert!(
        lines.iter().all(|line| !line.contains('\u{1b}')),
        "timeline should render styled text, not raw ANSI sequences"
    );
}

#[test]
fn timeline_entries_have_identity_markers() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
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
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

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
    let backend = TestBackend::new(100, 35);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.picker.open();

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

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

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

#[test]
fn render_to_lines_returns_empty_when_dimensions_are_zero() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    assert!(render_to_lines_for_test(&mut app, 0, 20).is_empty());
    assert!(render_to_lines_for_test(&mut app, 20, 0).is_empty());
}

#[test]
fn render_to_lines_handles_wide_unicode_cells() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.apply_agent_event_for_test(blazar::agent::protocol::AgentEvent::TextDelta {
        text: "Emoji 😀 output and 你好".into(),
    });

    let lines = render_to_lines_for_test(&mut app, 60, 20);
    let text = lines.join("\n");
    assert!(text.contains('😀'));
}

#[test]
fn render_frame_handles_streaming_indicator_in_tight_layouts() {
    let backend = TestBackend::new(3, 6);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.apply_agent_event_for_test(blazar::agent::protocol::AgentEvent::TurnStarted {
        turn_id: "tight-stream".into(),
    });

    terminal
        .draw(|frame| render_frame(frame, &mut app, 0))
        .expect("render should succeed even when streaming area is narrow");
}

#[test]
fn chat_view_renders_pending_user_rows_while_busy() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.apply_agent_event_for_test(blazar::agent::protocol::AgentEvent::TurnStarted {
        turn_id: "busy-turn".into(),
    });
    app.send_message("queued while busy");

    let lines = render_to_lines_for_test(&mut app, 100, 35);
    let text = lines.join("\n");

    assert!(
        text.contains("queued while busy (pending)"),
        "queued user text should render as a pending timeline row while the agent is busy"
    );
}

#[test]
fn pending_row_disappears_after_queue_dispatch() {
    use blazar::agent::protocol::AgentEvent;

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "busy-turn".into(),
    });
    app.send_message("queued while busy");

    let busy_lines = render_to_lines_for_test(&mut app, 100, 35);
    assert!(
        busy_lines
            .iter()
            .any(|line| line.contains("queued while busy (pending)")),
        "pending row should be visible before dispatch"
    );

    app.apply_agent_event_for_test(AgentEvent::TurnComplete);
    app.tick();

    let after_lines = render_to_lines_for_test(&mut app, 100, 35);
    assert!(
        after_lines
            .iter()
            .all(|line| !line.contains("queued while busy (pending)")),
        "pending row should disappear after the queued turn is dispatched"
    );
    assert!(
        after_lines
            .iter()
            .any(|line| line.contains("queued while busy")),
        "dispatched queued message should remain in the timeline"
    );
}

#[test]
fn chat_view_renders_thinking_entries() {
    use blazar::agent::protocol::AgentEvent;

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: "internal reasoning should render".into(),
    });

    let lines = render_to_lines_for_test(&mut app, 100, 35);
    let text = lines.join("\n");
    assert!(
        text.contains("internal reasoning should render"),
        "thinking rows should render in the timeline surface"
    );
}

#[test]
fn chat_view_does_not_render_turn_separator_lines() {
    fn is_turn_separator(line: &str) -> bool {
        let trimmed = line.trim();
        line.starts_with("  ") && !trimmed.is_empty() && trimmed.chars().all(|ch| ch == '─')
    }

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.send_message("separator check");
    std::thread::sleep(std::time::Duration::from_millis(200));
    app.tick();

    let lines = render_to_lines_for_test(&mut app, 100, 35);
    for (index, line) in lines.iter().enumerate() {
        if !line.contains("Blazar #") && !line.contains("You #") {
            continue;
        }
        let has_separator_before = lines[..index]
            .iter()
            .rev()
            .find(|candidate| !candidate.trim().is_empty())
            .is_some_and(|candidate| is_turn_separator(candidate));
        assert!(
            !has_separator_before,
            "timeline should not insert turn separator rows before headers"
        );
    }
    assert!(
        lines.iter().all(|line| !is_turn_separator(line)),
        "timeline should not insert horizontal separator rows between entries"
    );
}
