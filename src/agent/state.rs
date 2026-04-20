use super::protocol::AgentEvent;

/// The state of the current turn in the agent loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnState {
    Idle,
    Streaming { turn_id: String },
    Done,
    Failed { error: String },
}

/// Blazar-owned agent runtime state.
///
/// Lives in `ChatApp` as product state — never in rendering helpers.
/// Follows the ReAct + state-machine pattern: each event from the
/// runtime drives a deterministic state transition.
#[derive(Debug)]
pub struct AgentRuntimeState {
    pub turn_state: TurnState,
    pub turn_count: u64,
    /// Partial response text accumulated during the current streaming turn.
    pub streaming_text: String,
    pub active_tool_name: Option<String>,
    pub tool_call_count: u64,
}

impl Default for AgentRuntimeState {
    fn default() -> Self {
        Self {
            turn_state: TurnState::Idle,
            turn_count: 0,
            streaming_text: String::new(),
            active_tool_name: None,
            tool_call_count: 0,
        }
    }
}

impl AgentRuntimeState {
    /// Apply an agent event, returning `true` if the turn state enum changed.
    pub fn apply_event(&mut self, event: &AgentEvent) -> bool {
        match event {
            AgentEvent::TurnStarted { turn_id } => {
                self.turn_state = TurnState::Streaming {
                    turn_id: turn_id.clone(),
                };
                self.turn_count += 1;
                self.streaming_text.clear();
                self.active_tool_name = None;
                true
            }
            AgentEvent::TextDelta { text } => {
                self.streaming_text.push_str(text);
                false
            }
            AgentEvent::ThinkingDelta { .. } => {
                // Thinking deltas don't accumulate in state — only in timeline.
                false
            }
            AgentEvent::ToolCallStarted { tool_name, .. } => {
                self.active_tool_name = Some(tool_name.clone());
                self.tool_call_count += 1;
                false
            }
            AgentEvent::ToolCallCompleted { .. } => {
                self.active_tool_name = None;
                false
            }
            AgentEvent::TurnComplete => {
                self.turn_state = TurnState::Done;
                self.active_tool_name = None;
                true
            }
            AgentEvent::TurnFailed { error } => {
                self.turn_state = TurnState::Failed {
                    error: error.clone(),
                };
                self.active_tool_name = None;
                true
            }
        }
    }

    /// Whether the agent is currently processing a turn.
    pub fn is_busy(&self) -> bool {
        matches!(self.turn_state, TurnState::Streaming { .. })
    }
}
