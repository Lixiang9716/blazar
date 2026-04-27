use super::protocol::AgentEvent;
use crate::agent::tools::ToolKind;
use log::warn;

/// The state of the current turn in the agent loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnState {
    Idle,
    Streaming { turn_id: String },
    Done,
    Failed { error: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveToolStatus {
    Running,
    Success,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveTool {
    pub call_id: String,
    pub tool_name: String,
    pub kind: ToolKind,
    pub status: ActiveToolStatus,
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
    pub active_tools: Vec<ActiveTool>,
    pub tool_call_count: u64,
}

impl Default for AgentRuntimeState {
    fn default() -> Self {
        Self {
            turn_state: TurnState::Idle,
            turn_count: 0,
            streaming_text: String::new(),
            active_tools: Vec::new(),
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
                self.active_tools.clear();
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
            AgentEvent::UsageUpdated { .. } => false,
            AgentEvent::ToolCallStarted {
                call_id,
                tool_name,
                kind,
                ..
            } => {
                if self
                    .active_tools
                    .iter()
                    .find(|active_tool| active_tool.call_id == *call_id)
                    .is_some()
                {
                    warn!(
                        "agent state: duplicate ToolCallStarted for call_id={call_id} tool={tool_name}; ignoring duplicate start"
                    );
                } else {
                    self.active_tools.push(ActiveTool {
                        call_id: call_id.clone(),
                        tool_name: tool_name.clone(),
                        kind: *kind,
                        status: ActiveToolStatus::Running,
                    });
                    self.tool_call_count += 1;
                }
                false
            }
            AgentEvent::ToolCallCompleted {
                call_id, is_error, ..
            } => {
                if let Some(active_tool) = self
                    .active_tools
                    .iter_mut()
                    .find(|active_tool| active_tool.call_id == *call_id)
                {
                    active_tool.status = if *is_error {
                        ActiveToolStatus::Error
                    } else {
                        ActiveToolStatus::Success
                    };
                } else {
                    warn!(
                        "agent state: ToolCallCompleted for unknown call_id={call_id}; ignoring completion"
                    );
                }
                false
            }
            AgentEvent::AcpAgentsRefreshed | AgentEvent::AcpAgentsRefreshFailed { .. } => false,
            AgentEvent::TurnComplete => {
                self.turn_state = TurnState::Done;
                self.active_tools.clear();
                true
            }
            AgentEvent::TurnFailed { error, .. } => {
                self.turn_state = TurnState::Failed {
                    error: error.clone(),
                };
                self.active_tools.clear();
                true
            }
        }
    }

    /// Whether the agent is currently processing a turn.
    pub fn is_busy(&self) -> bool {
        matches!(self.turn_state, TurnState::Streaming { .. })
    }
}
