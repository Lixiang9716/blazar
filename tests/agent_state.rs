use blazar::agent::protocol::AgentEvent;
use blazar::agent::state::{AgentRuntimeState, TurnState};

#[test]
fn idle_by_default() {
    let state = AgentRuntimeState::default();
    assert_eq!(state.turn_state, TurnState::Idle);
    assert_eq!(state.turn_count, 0);
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
