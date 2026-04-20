pub mod echo;
pub mod siliconflow;

use std::sync::mpsc::Sender;

/// Events emitted by a provider during a single turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderEvent {
    /// A chunk of generated text.
    TextDelta(String),
    /// A chunk of chain-of-thought reasoning (thinking mode).
    ThinkingDelta(String),
    /// The model requests tool/function calls (JSON-serialized `Vec<ToolCall>`).
    ToolCallRequest(String),
    /// The provider finished generating.
    TurnComplete,
    /// The provider encountered an error.
    Error(String),
}

/// Trait for LLM providers.
///
/// A provider receives a user prompt and streams `ProviderEvent`s through
/// the supplied channel.  Implementations run on a background thread so
/// blocking I/O (HTTP, sleep) is acceptable.
///
/// `Send + Sync` is required so the provider can be shared across the
/// runtime thread and its scoped sub-threads.
pub trait LlmProvider: Send + Sync + 'static {
    fn stream_turn(&self, prompt: &str, tx: Sender<ProviderEvent>);
}
