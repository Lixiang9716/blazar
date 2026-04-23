pub mod echo;
pub mod openai_compat;
pub mod openrouter;

use crate::agent::tools::ToolSpec;
use std::sync::mpsc::Sender;

/// A model entry returned by a provider's model-listing API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub description: String,
}

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
/// A provider represents a **connection** to an inference backend
/// (API key + base URL).  The specific model is selected per-call
/// via the `model` parameter on `stream_turn`, so a single provider
/// instance can serve any model it supports.
///
/// `Send + Sync` is required so the provider can be shared across the
/// runtime thread and its scoped sub-threads.
pub trait LlmProvider: Send + Sync + 'static {
    /// Stream a single turn using the given `model`.
    fn stream_turn(
        &self,
        model: &str,
        messages: &[ProviderMessage],
        tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    );

    /// List models available from this provider.
    ///
    /// Implementations should query the remote API.  Returns an empty vec
    /// for providers that don't support model listing (e.g. `EchoProvider`).
    fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        Ok(Vec::new())
    }
}

// ── Provider factory ────────────────────────────────────────────────

/// Load the configured provider and default model from `config/provider.json`,
/// falling back to `EchoProvider` when the config is missing or invalid.
pub fn load_provider(repo_root: &str) -> (Box<dyn LlmProvider>, String) {
    match openai_compat::OpenAiConfig::load(repo_root) {
        Ok(cfg) => {
            let name = cfg.model.clone();
            let is_openrouter = cfg
                .provider_type
                .as_deref()
                .is_some_and(|t| t.eq_ignore_ascii_case("openrouter"));
            if is_openrouter {
                (Box::new(openrouter::OpenRouterProvider::new(cfg)), name)
            } else {
                (Box::new(openai_compat::OpenAiProvider::new(cfg)), name)
            }
        }
        Err(_) => (Box::new(echo::EchoProvider::default()), "echo".to_owned()),
    }
}

/// Return models available from the configured provider.
pub fn available_models(repo_root: &str) -> Vec<ModelInfo> {
    let repo_root = repo_root.to_owned();
    std::thread::spawn(move || {
        let (provider, _) = load_provider(&repo_root);
        provider.list_models().unwrap_or_default()
    })
    .join()
    .unwrap_or_default()
}
