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

/// Port abstraction for provider configuration queries.
///
/// Decouples model-metadata and UI layers from the concrete file-based
/// provider config so that tests can supply canned data without
/// thread-local hacks.
pub trait ProviderConfigPort: Send + Sync {
    /// Return models available from the configured provider.
    fn available_models(&self, repo_root: &str) -> Vec<ModelInfo>;

    /// Return the user-configured max_tokens from provider config.
    fn configured_max_tokens(&self, repo_root: &str) -> Option<u32>;

    /// Resolve context length for a specific model from a cached model list.
    fn resolve_model_context_length(&self, models: &[ModelInfo], model_id: &str) -> Option<u32> {
        resolve_model_context_length_from_models(models, model_id)
    }
}

/// Default implementation that delegates to the module-level free functions.
pub struct DefaultProviderConfig;

impl ProviderConfigPort for DefaultProviderConfig {
    fn available_models(&self, repo_root: &str) -> Vec<ModelInfo> {
        available_models(repo_root)
    }

    fn configured_max_tokens(&self, repo_root: &str) -> Option<u32> {
        configured_max_tokens(repo_root)
    }
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
            let provider: Result<Box<dyn LlmProvider>, String> = if is_openrouter {
                openrouter::OpenRouterProvider::new(cfg).map(|p| Box::new(p) as _)
            } else {
                openai_compat::OpenAiProvider::new(cfg).map(|p| Box::new(p) as _)
            };
            match provider {
                Ok(p) => (p, name),
                Err(e) => {
                    log::error!("provider init failed, falling back to echo: {e}");
                    (Box::new(echo::EchoProvider::default()), "echo".to_owned())
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_provider_config_available_models_returns_vec() {
        let config = DefaultProviderConfig;
        // With test behavior set, we can control the return value.
        let models = with_available_models_behavior_for_test(
            AvailableModelsTestBehavior::Return(vec![ModelInfo {
                id: "test-model".into(),
                description: "A test model".into(),
                context_length: Some(4096),
            }]),
            || config.available_models("."),
        );
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "test-model");
    }

    #[test]
    fn default_provider_config_configured_max_tokens() {
        let config = DefaultProviderConfig;
        // Without a valid config file, this returns None.
        let result = config.configured_max_tokens("/nonexistent/path");
        assert!(result.is_none());
    }

    #[test]
    fn resolve_model_context_length_finds_matching_model() {
        let models = vec![
            ModelInfo {
                id: "gpt-4".into(),
                description: "GPT-4".into(),
                context_length: Some(8192),
            },
            ModelInfo {
                id: "gpt-3.5".into(),
                description: "GPT-3.5".into(),
                context_length: Some(4096),
            },
        ];
        let config = DefaultProviderConfig;
        assert_eq!(
            config.resolve_model_context_length(&models, "gpt-4"),
            Some(8192)
        );
        assert_eq!(
            config.resolve_model_context_length(&models, "gpt-3.5"),
            Some(4096)
        );
        assert_eq!(
            config.resolve_model_context_length(&models, "unknown"),
            None
        );
    }

    #[test]
    fn resolve_model_context_length_free_fn() {
        let models = vec![ModelInfo {
            id: "m1".into(),
            description: "Model 1".into(),
            context_length: Some(2048),
        }];
        assert_eq!(
            resolve_model_context_length_from_models(&models, "m1"),
            Some(2048)
        );
        assert_eq!(
            resolve_model_context_length_from_models(&models, "missing"),
            None
        );
    }

    #[test]
    fn resolve_model_context_length_integration() {
        // Uses the top-level resolve_model_context_length which calls available_models.
        let result = with_available_models_behavior_for_test(
            AvailableModelsTestBehavior::Return(vec![ModelInfo {
                id: "test-model".into(),
                description: "test".into(),
                context_length: Some(16384),
            }]),
            || resolve_model_context_length(".", "test-model"),
        );
        assert_eq!(result, Some(16384));
    }

    #[test]
    fn echo_provider_implements_llm_provider_trait() {
        let provider = echo::EchoProvider::default();
        // list_models default returns empty vec
        let models = provider.list_models().unwrap();
        assert!(models.is_empty());
    }
}
