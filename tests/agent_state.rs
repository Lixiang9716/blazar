use blazar::agent::protocol::AgentEvent;
use blazar::agent::state::{ActiveToolStatus, AgentRuntimeState, TurnState};
use blazar::agent::tools::ToolKind;

#[test]
fn idle_by_default() {
    let state = AgentRuntimeState::default();
    assert_eq!(state.turn_state, TurnState::Idle);
    assert_eq!(state.turn_count, 0);
    assert!(state.active_tools.is_empty());
    assert_eq!(state.tool_call_count, 0);
    assert!(!state.is_busy());
}

#[test]
fn turn_started_transitions_to_streaming() {
    let mut state = AgentRuntimeState::default();
    let changed = state.apply_event(&AgentEvent::TurnStarted {
        turn_id: "turn-1".into(),
    });
    assert!(changed);
    assert_eq!(
        state.turn_state,
        TurnState::Streaming {
            turn_id: "turn-1".into()
        }
    );
    assert_eq!(state.turn_count, 1);
    assert!(state.is_busy());
}

#[test]
fn text_delta_accumulates() {
    let mut state = AgentRuntimeState::default();
    state.apply_event(&AgentEvent::TurnStarted {
        turn_id: "turn-1".into(),
    });
    let changed = state.apply_event(&AgentEvent::TextDelta {
        text: "Hello".into(),
    });
    assert!(!changed); // TextDelta doesn't change turn_state enum
    assert_eq!(state.streaming_text, "Hello");

    state.apply_event(&AgentEvent::TextDelta {
        text: " world".into(),
    });
    assert_eq!(state.streaming_text, "Hello world");
}

#[test]
fn turn_complete_transitions_to_done() {
    let mut state = AgentRuntimeState::default();
    state.apply_event(&AgentEvent::TurnStarted {
        turn_id: "turn-1".into(),
    });
    let changed = state.apply_event(&AgentEvent::TurnComplete);
    assert!(changed);
    assert_eq!(state.turn_state, TurnState::Done);
    assert!(!state.is_busy());
}

#[test]
fn turn_failed_captures_error() {
    let mut state = AgentRuntimeState::default();
    state.apply_event(&AgentEvent::TurnStarted {
        turn_id: "turn-1".into(),
    });
    let changed = state.apply_event(&AgentEvent::TurnFailed {
        error: "timeout".into(),
    });
    assert!(changed);
    assert_eq!(
        state.turn_state,
        TurnState::Failed {
            error: "timeout".into()
        }
    );
    assert!(!state.is_busy());
}

#[test]
fn new_turn_clears_streaming_text() {
    let mut state = AgentRuntimeState::default();
    state.apply_event(&AgentEvent::TurnStarted {
        turn_id: "turn-1".into(),
    });
    state.apply_event(&AgentEvent::TextDelta {
        text: "accumulated".into(),
    });
    assert!(!state.streaming_text.is_empty());

    // Start a second turn — text should be cleared.
    state.apply_event(&AgentEvent::TurnStarted {
        turn_id: "turn-2".into(),
    });
    assert!(state.streaming_text.is_empty());
    assert_eq!(state.turn_count, 2);
}

#[test]
fn thinking_delta_does_not_change_state() {
    let mut state = AgentRuntimeState::default();
    state.apply_event(&AgentEvent::TurnStarted {
        turn_id: "turn-1".into(),
    });
    let changed = state.apply_event(&AgentEvent::ThinkingDelta {
        text: "reasoning...".into(),
    });
    assert!(!changed, "ThinkingDelta should not change turn_state");
    assert!(
        state.streaming_text.is_empty(),
        "ThinkingDelta should not accumulate in streaming_text"
    );
}

#[test]
fn tool_call_events_track_multiple_active_tools_by_call_id() {
    let mut state = AgentRuntimeState::default();
    state.apply_event(&AgentEvent::TurnStarted {
        turn_id: "turn-1".into(),
    });

    let changed = state.apply_event(&AgentEvent::ToolCallStarted {
        call_id: "call-1".into(),
        tool_name: "read_file".into(),
        kind: ToolKind::Local,
        arguments: "{\"path\":\"Cargo.toml\"}".into(),
    });
    assert!(!changed, "ToolCallStarted should not change turn_state");

    state.apply_event(&AgentEvent::ToolCallStarted {
        call_id: "call-2".into(),
        tool_name: "delegate".into(),
        kind: ToolKind::Agent,
        arguments: "{\"prompt\":\"review\"}".into(),
    });

    assert_eq!(state.tool_call_count, 2);
    assert_eq!(state.active_tools.len(), 2);
    assert_eq!(state.active_tools[0].call_id, "call-1");
    assert_eq!(state.active_tools[0].tool_name, "read_file");
    assert_eq!(state.active_tools[0].kind, ToolKind::Local);
    assert_eq!(state.active_tools[0].status, ActiveToolStatus::Running);
    assert_eq!(state.active_tools[1].call_id, "call-2");
    assert_eq!(state.active_tools[1].tool_name, "delegate");
    assert_eq!(state.active_tools[1].kind, ToolKind::Agent);
    assert_eq!(state.active_tools[1].status, ActiveToolStatus::Running);

    let changed = state.apply_event(&AgentEvent::ToolCallCompleted {
        call_id: "call-1".into(),
        output: "package".into(),
        is_error: false,
    });
    assert!(!changed, "ToolCallCompleted should not change turn_state");
    assert_eq!(state.active_tools.len(), 2);
    assert_eq!(state.active_tools[0].call_id, "call-1");
    assert_eq!(state.active_tools[0].status, ActiveToolStatus::Success);
    assert_eq!(state.active_tools[1].call_id, "call-2");
    assert_eq!(state.active_tools[1].status, ActiveToolStatus::Running);
    assert!(state.is_busy());
}

#[test]
fn duplicate_tool_call_started_is_ignored_without_overwriting_active_tool() {
    let mut state = AgentRuntimeState::default();
    state.apply_event(&AgentEvent::TurnStarted {
        turn_id: "turn-1".into(),
    });
    state.apply_event(&AgentEvent::ToolCallStarted {
        call_id: "call-1".into(),
        tool_name: "read_file".into(),
        kind: ToolKind::Local,
        arguments: "{\"path\":\"Cargo.toml\"}".into(),
    });

    state.apply_event(&AgentEvent::ToolCallStarted {
        call_id: "call-1".into(),
        tool_name: "delegate".into(),
        kind: ToolKind::Agent,
        arguments: "{\"prompt\":\"review\"}".into(),
    });

    assert_eq!(state.tool_call_count, 1);
    assert_eq!(state.active_tools.len(), 1);
    assert_eq!(state.active_tools[0].tool_name, "read_file");
    assert_eq!(state.active_tools[0].kind, ToolKind::Local);
    assert_eq!(state.active_tools[0].status, ActiveToolStatus::Running);
}

#[test]
fn unknown_tool_call_completed_is_ignored_without_changing_state() {
    let mut state = AgentRuntimeState::default();
    state.apply_event(&AgentEvent::TurnStarted {
        turn_id: "turn-1".into(),
    });
    state.apply_event(&AgentEvent::ToolCallStarted {
        call_id: "call-1".into(),
        tool_name: "read_file".into(),
        kind: ToolKind::Local,
        arguments: "{\"path\":\"Cargo.toml\"}".into(),
    });

    let changed = state.apply_event(&AgentEvent::ToolCallCompleted {
        call_id: "missing".into(),
        output: "ignored".into(),
        is_error: true,
    });

    assert!(!changed);
    assert_eq!(state.tool_call_count, 1);
    assert_eq!(state.active_tools.len(), 1);
    assert_eq!(state.active_tools[0].call_id, "call-1");
    assert_eq!(state.active_tools[0].status, ActiveToolStatus::Running);
    assert!(state.is_busy());
}

#[test]
fn full_lifecycle_idle_streaming_done_idle() {
    let mut state = AgentRuntimeState::default();
    assert_eq!(state.turn_state, TurnState::Idle);

    // Turn 1: start → stream → complete
    state.apply_event(&AgentEvent::TurnStarted {
        turn_id: "turn-1".into(),
    });
    assert!(state.is_busy());

    state.apply_event(&AgentEvent::TextDelta {
        text: "hello".into(),
    });
    assert_eq!(state.streaming_text, "hello");

    state.apply_event(&AgentEvent::TurnComplete);
    assert_eq!(state.turn_state, TurnState::Done);

    // Turn 2: start → fail
    state.apply_event(&AgentEvent::TurnStarted {
        turn_id: "turn-2".into(),
    });
    assert!(state.streaming_text.is_empty());
    assert_eq!(state.turn_count, 2);

    state.apply_event(&AgentEvent::TurnFailed {
        error: "timeout".into(),
    });
    assert!(!state.is_busy());
    assert_eq!(
        state.turn_state,
        TurnState::Failed {
            error: "timeout".into()
        }
    );
}
