use crate::agent::tools::ToolKind;
use crate::agent::runtime::RuntimeErrorKind;

/// Commands sent from the UI thread to the agent runtime.
pub enum AgentCommand {
    /// Submit a new turn with the given user prompt.
    SubmitTurn { prompt: String },
    /// Switch the active model without rebuilding the runtime.
    SetModel { model: String },
    /// Refresh ACP-discovered agents and rebuild runtime tool registry.
    RefreshAcpAgents,
    /// Cancel the current turn (future use).
    Cancel,
    /// Shut down the runtime thread.
    Shutdown,
}

/// Events sent from the agent runtime back to the UI thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    /// A new turn has started processing.
    TurnStarted { turn_id: String },
    /// A chunk of assistant text was generated (streaming).
    TextDelta { text: String },
    /// A chunk of chain-of-thought reasoning (thinking mode).
    ThinkingDelta { text: String },
    /// A tool call is about to execute.
    ToolCallStarted {
        call_id: String,
        tool_name: String,
        kind: ToolKind,
        arguments: String,
    },
    /// A tool call finished executing.
    ToolCallCompleted {
        call_id: String,
        output: String,
        is_error: bool,
    },
    /// ACP agent discovery finished and tools were refreshed.
    AcpAgentsRefreshed,
    /// ACP agent discovery failed.
    AcpAgentsRefreshFailed { error: String },
    /// The current turn completed successfully.
    TurnComplete,
    /// The current turn failed.
    TurnFailed { kind: RuntimeErrorKind, error: String },
}
