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
    pub context_length: Option<u32>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
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
    /// Updated provider token usage for the current turn.
    Usage(ProviderUsage),
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

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvailableModelsTestBehavior {
    Return(Vec<ModelInfo>),
    DelayedReturn {
        models: Vec<ModelInfo>,
        delay: std::time::Duration,
    },
    Panic(&'static str),
}

#[cfg(test)]
thread_local! {
    static AVAILABLE_MODELS_TEST_BEHAVIOR: std::cell::RefCell<Option<AvailableModelsTestBehavior>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) struct AvailableModelsTestBehaviorGuard;

#[cfg(test)]
impl Drop for AvailableModelsTestBehaviorGuard {
    fn drop(&mut self) {
        AVAILABLE_MODELS_TEST_BEHAVIOR.with(|behavior| {
            *behavior.borrow_mut() = None;
        });
    }
}

#[cfg(test)]
pub(crate) fn current_available_models_behavior_for_test() -> Option<AvailableModelsTestBehavior> {
    AVAILABLE_MODELS_TEST_BEHAVIOR.with(|slot| slot.borrow().clone())
}

#[cfg(test)]
pub(crate) fn set_available_models_behavior_for_current_thread_for_test(
    behavior: Option<AvailableModelsTestBehavior>,
) -> AvailableModelsTestBehaviorGuard {
    AVAILABLE_MODELS_TEST_BEHAVIOR.with(|slot| {
        *slot.borrow_mut() = behavior;
    });
    AvailableModelsTestBehaviorGuard
}

#[cfg(test)]
pub(crate) fn with_available_models_behavior_for_test<T>(
    behavior: AvailableModelsTestBehavior,
    f: impl FnOnce() -> T,
) -> T {
    let _reset = set_available_models_behavior_for_current_thread_for_test(Some(behavior));
    f()
}

/// Return models available from the configured provider.
pub fn available_models(repo_root: &str) -> Vec<ModelInfo> {
    #[cfg(test)]
    if let Some(behavior) = AVAILABLE_MODELS_TEST_BEHAVIOR.with(|slot| slot.borrow().clone()) {
        return match behavior {
            AvailableModelsTestBehavior::Return(models) => models,
            AvailableModelsTestBehavior::DelayedReturn { models, delay } => {
                std::thread::sleep(delay);
                models
            }
            AvailableModelsTestBehavior::Panic(message) => panic!("{message}"),
        };
    }

    let repo_root = repo_root.to_owned();
    std::thread::spawn(move || {
        let (provider, _) = load_provider(&repo_root);
        provider.list_models().unwrap_or_default()
    })
    .join()
    .unwrap_or_default()
}

pub fn resolve_model_context_length_from_models(
    models: &[ModelInfo],
    model_id: &str,
) -> Option<u32> {
    models
        .iter()
        .find(|model| model.id == model_id)
        .and_then(|model| model.context_length)
}

pub fn resolve_model_context_length(repo_root: &str, model_id: &str) -> Option<u32> {
    let models = available_models(repo_root);
    resolve_model_context_length_from_models(&models, model_id)
}

pub fn configured_max_tokens(repo_root: &str) -> Option<u32> {
    openai_compat::OpenAiConfig::load(repo_root)
        .ok()
        .map(|cfg| cfg.max_tokens)
}
