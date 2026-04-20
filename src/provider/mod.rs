pub mod echo;
pub mod siliconflow;

use crate::agent::tools::ToolSpec;
use std::sync::mpsc::Sender;

/// Conversation history replayed into a provider pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderMessage {
    User {
        content: String,
    },
    Assistant {
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        tool_call_id: String,
        output: String,
        is_error: bool,
    },
}

/// Events emitted by a provider during a single turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderEvent {
    /// A chunk of generated text.
    TextDelta(String),
    /// A chunk of chain-of-thought reasoning (thinking mode).
    ThinkingDelta(String),
    /// The model requests a tool/function call.
    ToolCall {
        call_id: String,
        name: String,
        arguments: String,
    },
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
    fn stream_turn(
        &self,
        messages: &[ProviderMessage],
        tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    );
}
