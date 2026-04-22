/// Commands sent from the UI thread to the agent runtime.
pub enum AgentCommand {
    /// Submit a new turn with the given user prompt.
    SubmitTurn { prompt: String },
    /// Switch the active model without rebuilding the runtime.
    SetModel { model: String },
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
        arguments: String,
    },
    /// A tool call finished executing.
    ToolCallCompleted {
        call_id: String,
        output: String,
        is_error: bool,
    },
    /// The current turn completed successfully.
    TurnComplete,
    /// The current turn failed.
    TurnFailed { error: String },
}
