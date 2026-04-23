use blazar::agent::protocol::AgentEvent;
use blazar::agent::tools::ToolKind;
use blazar::chat::app::ChatApp;
use blazar::chat::input::InputAction;
use blazar::chat::view::render_to_lines_for_test;

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn completed_tool_call_renders_summary_in_timeline() {
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
        output: "package\nname = \"blazar\"".into(),
        is_error: false,
    });

    let lines = render_to_lines_for_test(&mut app, 100, 35);
    assert!(lines.iter().any(|line| line.contains("read_file")));
    assert!(lines.iter().any(|line| line.contains("package")));
    assert!(lines.iter().any(|line| line.contains("name = \"blazar\"")));
}

#[test]
fn toggle_details_reveals_full_tool_output() {
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
        output: "package\nname = \"blazar\"".into(),
        is_error: false,
    });

    app.handle_action(InputAction::ToggleDetails);
    let lines = render_to_lines_for_test(&mut app, 100, 35);

    assert!(lines.iter().any(|line| line.contains("name = \"blazar\"")));
}

#[test]
fn ctrl_o_details_path_keeps_full_tool_result_text() {
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
        output: "package\nname = \"blazar\"\nversion = \"0.1.0\"".into(),
        is_error: false,
    });

    app.handle_action(InputAction::ToggleDetails);
    let lines = render_to_lines_for_test(&mut app, 100, 35);

    assert!(lines.iter().any(|line| line.contains("package")));
    assert!(lines.iter().any(|line| line.contains("name = \"blazar\"")));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("version = \"0.1.0\""))
    );
}

#[test]
fn completed_tool_call_preview_uses_two_lines_from_full_output() {
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
        output: "package\nname = \"blazar\"\nversion = \"0.1.0\"".into(),
        is_error: false,
    });

    let lines = render_to_lines_for_test(&mut app, 100, 35);
    assert!(lines.iter().any(|line| line.contains("package")));
    assert!(lines.iter().any(|line| line.contains("name = \"blazar\"")));
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("version = \"0.1.0\""))
    );
}

#[test]
fn running_tool_call_uses_app_formatted_argument_details_for_subtitle() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-1".into(),
        tool_name: "read_file".into(),
        kind: ToolKind::Local,
        arguments: "{\"path\":\"Cargo.toml\"}".into(),
        batch_id: 7,
        replay_index: 0,
        normalized_claims: Vec::new(),
    });

    let lines = render_to_lines_for_test(&mut app, 100, 35);
    assert!(lines.iter().any(|line| line.contains("Cargo.toml")));
    assert!(!lines.iter().any(|line| line.contains("invalid args")));
}

#[test]
fn completed_tool_call_does_not_overtrigger_invalid_args_on_output_details() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-1".into(),
        tool_name: "read_file".into(),
        kind: ToolKind::Local,
        arguments: "{\"path\":\"Cargo.toml\"}".into(),
        batch_id: 7,
        replay_index: 0,
        normalized_claims: Vec::new(),
    });
    app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
        call_id: "call-1".into(),
        output: "package\nname = \"blazar\"".into(),
        is_error: false,
    });

    let lines = render_to_lines_for_test(&mut app, 100, 35);
    assert!(!lines.iter().any(|line| line.contains("invalid args")));
}
