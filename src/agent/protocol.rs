/// Commands sent from the UI thread to the agent runtime.
pub enum AgentCommand {
    /// Submit a new turn with the given user prompt.
    SubmitTurn { prompt: String },
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
    /// The model requests tool/function calls (JSON-serialized).
    ToolCallRequest { payload: String },
    /// The current turn completed successfully.
    TurnComplete,
    /// The current turn failed.
    TurnFailed { error: String },
}
