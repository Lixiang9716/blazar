use crate::agent::tools::ToolKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    ProviderTransient,
    ProviderFatal,
    ProtocolInvalidPayload,
    ToolExecution,
    Cancelled,
}

impl RuntimeErrorKind {
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::ProviderTransient)
    }
}

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
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
    /// Updated provider token usage for the current turn.
    UsageUpdated(AgentUsage),
    /// A tool call is about to execute.
    ToolCallStarted {
        call_id: String,
        tool_name: String,
        kind: ToolKind,
        arguments: String,
        batch_id: u32,
        replay_index: usize,
        normalized_claims: Vec<String>,
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
    TurnFailed {
        kind: RuntimeErrorKind,
        error: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_retryable_returns_true_only_for_provider_transient() {
        assert!(RuntimeErrorKind::ProviderTransient.is_retryable());
        assert!(!RuntimeErrorKind::ProviderFatal.is_retryable());
        assert!(!RuntimeErrorKind::ProtocolInvalidPayload.is_retryable());
        assert!(!RuntimeErrorKind::ToolExecution.is_retryable());
        assert!(!RuntimeErrorKind::Cancelled.is_retryable());
    }
}
