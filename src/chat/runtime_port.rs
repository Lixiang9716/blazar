use crate::agent::protocol::AgentEvent;

/// Runtime boundary trait used by chat to interact with the agent runtime.
/// Keeps a minimal, testable surface so the runtime can be swapped or mocked.
pub trait AgentRuntimePort {
    /// Submit a new turn to the runtime.
    fn submit_turn(&self, prompt: &str) -> Result<(), String>;

    /// Switch the active model.
    fn set_model(&self, model: &str) -> Result<(), String>;

    /// Refresh ACP-discovered agents and rebuild tool registry.
    fn refresh_acp_agents(&self) -> Result<(), String>;

    /// Cancel the current turn.
    fn cancel(&self);

    /// Non-blocking poll for the next event.
    fn try_recv(&self) -> Option<AgentEvent>;

    /// Test-only: shutdown runtime and join worker thread.
    #[cfg(test)]
    fn shutdown_for_test(&mut self);
}
