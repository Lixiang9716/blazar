use blazar::chat::app::ChatApp;
use blazar::chat::users_state::UsersLayoutPolicy;
use blazar::chat::view::render::contracts::{
    RenderCtx, RenderError, RenderRegistry, RenderSlot, RenderUnit,
};
use blazar::chat::view::{render_frame, render_to_lines_for_test};
use blazar::chat::view::{
    render_to_lines_for_test_with_registry, render_to_lines_for_test_with_users_policy,
};
use ratatui_core::{
    backend::TestBackend,
    layout::Rect,
    style::Color,
    terminal::{Frame, Terminal},
    text::Line,
};
use ratatui_widgets::paragraph::Paragraph;

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
fn chat_view_renders_model_row() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let lines = render_to_lines_for_test(&mut app, 100, 35);

    assert!(
        lines
            .iter()
            .any(|line| line.contains("AUTO") && line.contains("echo")),
        "model row should render mode and model name"
    );
}

#[test]
fn render_frame_dispatches_slots_without_direct_module_calls() {
    struct SlotTextUnit(&'static str);

    impl RenderUnit for SlotTextUnit {
        fn render(
            &self,
            frame: &mut Frame,
            area: Rect,
            _ctx: &mut RenderCtx<'_>,
        ) -> Result<(), RenderError> {
            if area.width == 0 || area.height == 0 {
                return Ok(());
            }

            frame.render_widget(Paragraph::new(Line::from(self.0)), area);
            Ok(())
        }
    }

    struct ProbeRegistry {
        timeline: SlotTextUnit,
        users_top: SlotTextUnit,
        users_input: SlotTextUnit,
        users_model: SlotTextUnit,
        users_top_input_separator: SlotTextUnit,
        users_input_model_separator: SlotTextUnit,
        picker_overlay: SlotTextUnit,
    }

    impl Default for ProbeRegistry {
        fn default() -> Self {
            Self {
                timeline: SlotTextUnit("slot:timeline"),
                users_top: SlotTextUnit("slot:users-top"),
                users_input: SlotTextUnit("slot:users-input"),
                users_model: SlotTextUnit("slot:users-model"),
                users_top_input_separator: SlotTextUnit("slot:users-top-input-separator"),
                users_input_model_separator: SlotTextUnit("slot:users-input-model-separator"),
                picker_overlay: SlotTextUnit("slot:picker-overlay"),
            }
        }
    }

    impl RenderRegistry for ProbeRegistry {
        fn resolve(&self, slot: RenderSlot) -> Option<&dyn RenderUnit> {
            match slot {
                RenderSlot::Timeline => Some(&self.timeline),
                RenderSlot::UsersTop => Some(&self.users_top),
                RenderSlot::UsersInput => Some(&self.users_input),
                RenderSlot::UsersModel => Some(&self.users_model),
                RenderSlot::UsersTopInputSeparator => Some(&self.users_top_input_separator),
                RenderSlot::UsersInputModelSeparator => Some(&self.users_input_model_separator),
                RenderSlot::PickerOverlay => Some(&self.picker_overlay),
            }
        }
    }

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let lines = render_to_lines_for_test_with_registry(
        &mut app,
        100,
        24,
        UsersLayoutPolicy::default(),
        &ProbeRegistry::default(),
    );
    let text = lines.join("\n");

    assert!(text.contains("slot:timeline"));
    assert!(text.contains("slot:users-top"));
    assert!(text.contains("slot:users-model"));
}

#[test]
fn top_panel_renders_only_path_and_branch_in_normal_mode() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.set_pr_label_for_test(Some("PR#42 improve timeline".to_owned()));
    app.set_referenced_files_for_test(vec!["src/chat/view/mod.rs".to_owned()]);

    let policy = UsersLayoutPolicy {
        top_height: 1,
        input_height: 1,
        model_height: 2,
        max_command_window_size: 6,
    };
    let lines = render_to_lines_for_test_with_users_policy(&mut app, 130, 24, policy);
    let users_rows = &lines[lines.len().saturating_sub(6)..];

    assert!(
        users_rows[0].contains("~/blazar") && users_rows[0].contains("main"),
        "normal top panel should show path + branch"
    );
    assert!(
        !users_rows[0].contains("PR#42") && !users_rows[0].contains("src/chat/view/mod.rs"),
        "normal top panel should not show PR or reference metadata"
    );
}

#[test]
fn users_area_renders_top_input_model_with_separator() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let policy = UsersLayoutPolicy {
        top_height: 2,
        input_height: 1,
        model_height: 2,
        max_command_window_size: 6,
    };
    let lines = render_to_lines_for_test_with_users_policy(&mut app, 120, 24, policy);
    let users_rows = &lines[lines.len().saturating_sub(7)..];

    assert!(
        users_rows
            .iter()
            .any(|line| line.contains("~/blazar") && line.contains("main")),
        "top panel should show path + branch"
    );
    assert!(
        users_rows[2].contains("─") && users_rows[4].contains("─"),
        "separator rows should bracket the input panel"
    );
    assert!(
        users_rows[5].contains("AUTO") && users_rows[5].contains("echo"),
        "model panel should land after policy-sized top/input/separator rows"
    );
}

#[test]
fn users_area_hides_separator_when_input_or_model_is_zero_height() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

    let input_zero = UsersLayoutPolicy {
        top_height: 1,
        input_height: 0,
        model_height: 1,
        max_command_window_size: 6,
    };
    let input_zero_lines = render_to_lines_for_test_with_users_policy(&mut app, 120, 8, input_zero);
    assert!(
        input_zero_lines.iter().all(|line| !line.contains("─")),
        "separator should stay hidden when input height is zero"
    );

    let model_zero = UsersLayoutPolicy {
        top_height: 1,
        input_height: 1,
        model_height: 0,
        max_command_window_size: 6,
    };
    let model_zero_lines = render_to_lines_for_test_with_users_policy(&mut app, 120, 8, model_zero);
    assert!(
        model_zero_lines
            .iter()
            .filter(|line| line.contains("─"))
            .count()
            == 1,
        "top/input separator should still render when model height is zero"
    );
}

#[test]
fn chat_view_keeps_input_and_model_visible_in_tight_heights() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let lines = render_to_lines_for_test(&mut app, 100, 3);
    let users_rows = &lines[lines.len().saturating_sub(2)..];

    assert!(
        users_rows[0].contains("> ") && users_rows[0].contains("Describe a task"),
        "input panel should still render in tight heights"
    );
    assert!(
        users_rows[1].contains("AUTO") && users_rows[1].contains("echo"),
        "model panel should still render in tight heights"
    );
}

#[test]
fn mode_row_renders_context_ratio_when_available() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.set_context_usage_for_test(1200, 8000);

    let lines = render_to_lines_for_test(&mut app, 120, 24);
    let users_rows = &lines[lines.len().saturating_sub(5)..];

    assert!(
        users_rows[4].contains("1200/8000 (15%)"),
        "mode row should render context ratio when available"
    );
}

#[test]
fn slash_command_window_scroll_changes_visible_items() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    let policy = UsersLayoutPolicy {
        top_height: 4,
        input_height: 1,
        model_height: 1,
        max_command_window_size: 3,
    };
    let initial = render_to_lines_for_test_with_users_policy(&mut app, 100, 18, policy);
    let initial_rows = &initial[initial.len().saturating_sub(8)..];
    let initial_commands: Vec<&str> = initial_rows[1..4].iter().map(|line| line.trim()).collect();

    assert_eq!(initial_rows[0].trim(), "~/blazar · main");
    assert_eq!(initial_commands, vec!["• /help", "• /clear", "• /copy"]);

    for _ in 0..6 {
        app.handle_action(InputAction::ScrollDown);
    }
    let scrolled = render_to_lines_for_test_with_users_policy(&mut app, 100, 18, policy);
    let scrolled_rows = &scrolled[scrolled.len().saturating_sub(8)..];
    let scrolled_commands: Vec<&str> = scrolled_rows[1..4].iter().map(|line| line.trim()).collect();

    assert!(
        initial_commands != scrolled_commands,
        "scrolling should change the visible command rows"
    );
    assert_eq!(scrolled_rows[0].trim(), "~/blazar · main");
    assert_eq!(scrolled_commands[0], "• /mcp");
    assert!(scrolled_commands.iter().all(|line| !line.contains("/help")));
}

#[test]
fn top_panel_caps_command_window_to_policy_max_items() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    let policy = UsersLayoutPolicy {
        top_height: 8,
        input_height: 1,
        model_height: 1,
        max_command_window_size: 3,
    };
    let lines = render_to_lines_for_test_with_users_policy(&mut app, 100, 18, policy);
    let users_rows = &lines[lines.len().saturating_sub(8)..];

    assert_eq!(users_rows[0].trim(), "~/blazar · main");
    assert!(users_rows[1].contains("/help"));
    assert!(users_rows[2].contains("/clear"));
    assert!(users_rows[3].contains("/copy"));
    assert!(
        users_rows
            .iter()
            .take(4)
            .all(|line| !line.contains("/init")),
        "top panel should cap visible commands to the policy max"
    );
}

#[test]
fn default_policy_caps_command_window_to_six_items() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    let lines = render_to_lines_for_test(&mut app, 100, 18);
    let users_rows = &lines[lines.len().saturating_sub(11)..];

    assert!(users_rows[1].contains("/help"));
    assert!(users_rows[2].contains("/clear"));
    assert!(users_rows[3].contains("/copy"));
    assert!(users_rows[4].contains("/init"));
    assert!(users_rows[5].contains("/skills"));
    assert!(users_rows[6].contains("/model"));
    assert!(
        users_rows.iter().all(|line| !line.contains("/mcp")),
        "default policy should cap the visible window to 6 items"
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
fn picker_overlay_renders_via_registry_slot() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.picker.open();

    let lines = render_to_lines_for_test(&mut app, 100, 35);

    assert!(
        lines.iter().any(|line| line.contains("navigate")),
        "picker overlay footer should render via the picker registry slot"
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

#[test]
fn expanded_details_render_markdown_and_diff_blocks() {
    use blazar::agent::protocol::AgentEvent;
    use blazar::agent::tools::ToolKind;
    use blazar::chat::input::InputAction;

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-1".into(),
        tool_name: "read_file".into(),
        kind: ToolKind::Local,
        arguments: "{\"path\":\"Cargo.toml\"}".into(),
        batch_id: 0,
        replay_index: 0,
        normalized_claims: Vec::new(),
    });
    app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
        call_id: "call-1".into(),
        output: "```diff\n- old\n+ new\n```".into(),
        is_error: false,
    });
    app.handle_action(InputAction::ToggleDetails);
    let lines = render_to_lines_for_test(&mut app, 100, 24);
    let text = lines.join("\n");
    assert!(text.contains("- old"));
    assert!(text.contains("+ new"));
    assert!(!text.contains("```diff"));
}
