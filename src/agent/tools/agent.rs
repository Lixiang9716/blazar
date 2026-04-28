use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use super::{
    BuiltinToolDescriptor, BuiltinToolProfiles, Tool, ToolBuildContext, ToolBuildProfile, ToolKind,
    ToolResult, ToolRegistry, ToolSpec, register_builtin_tools,
};
use crate::agent::runtime::turn::{SilentObserver, TurnOutcome, execute_turn};
use crate::provider::{LlmProvider, ProviderMessage};

pub const AGENT_TOOL_NAME: &str = "sub_agent";
pub const AGENT_TOOL_DESCRIPTION: &str = "Delegate a task to a sub-agent that can read files, write files, list directories, and run bash commands. Use when the task is self-contained and benefits from independent reasoning.";

inventory::submit! {
    BuiltinToolDescriptor {
        name: AGENT_TOOL_NAME,
        profiles: BuiltinToolProfiles::MainOnly,
        build: |ctx: &ToolBuildContext| Box::new(AgentTool::new(
            AGENT_TOOL_NAME,
            AGENT_TOOL_DESCRIPTION,
            Arc::clone(&ctx.provider),
            &ctx.model,
            ctx.workspace_root.clone(),
        )),
    }
}

/// Runs a sub-agent turn. Abstracted so tests can provide a simple
/// implementation without invoking a real LLM provider.
pub(crate) trait TurnRunner: Send + Sync {
    fn run_turn(
        &self,
        messages: &mut Vec<ProviderMessage>,
        tools: &ToolRegistry,
        cancel_flag: &Arc<AtomicBool>,
    ) -> TurnOutcome;
}

/// Builds a tool registry for a sub-agent. Abstracted so tests can
/// skip real tool registration.
pub(crate) trait ToolRegistryFactory: Send + Sync {
    fn create(
        &self,
        ctx: &ToolBuildContext,
        profile: ToolBuildProfile,
    ) -> Result<ToolRegistry, String>;
}

/// Production implementation: calls `execute_turn` with `SilentObserver`.
struct DefaultTurnRunner {
    provider: Arc<dyn LlmProvider>,
    model: String,
}

impl TurnRunner for DefaultTurnRunner {
    fn run_turn(
        &self,
        messages: &mut Vec<ProviderMessage>,
        tools: &ToolRegistry,
        cancel_flag: &Arc<AtomicBool>,
    ) -> TurnOutcome {
        execute_turn(
            messages,
            &*self.provider,
            &self.model,
            tools,
            &SilentObserver,
            cancel_flag,
        )
    }
}

/// Production implementation: delegates to `register_builtin_tools`.
struct DefaultToolRegistryFactory;

impl ToolRegistryFactory for DefaultToolRegistryFactory {
    fn create(
        &self,
        ctx: &ToolBuildContext,
        profile: ToolBuildProfile,
    ) -> Result<ToolRegistry, String> {
        register_builtin_tools(ctx, profile)
    }
}

/// A tool that delegates work to a sub-agent turn.
///
/// When the parent agent invokes this tool, a fresh `execute_turn` is
/// run with [`SilentObserver`] so intermediate events are discarded.
/// The sub-agent has its own tool registry (workspace-scoped) and
/// returns its final assistant text as the tool result.
pub struct AgentTool {
    name: String,
    description: String,
    provider: Arc<dyn LlmProvider>,
    model: String,
    workspace_root: PathBuf,
    turn_runner: Box<dyn TurnRunner>,
    registry_factory: Box<dyn ToolRegistryFactory>,
}

impl AgentTool {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        provider: Arc<dyn LlmProvider>,
        model: impl Into<String>,
        workspace_root: PathBuf,
    ) -> Self {
        let model = model.into();
        Self {
            name: name.into(),
            description: description.into(),
            turn_runner: Box::new(DefaultTurnRunner {
                provider: Arc::clone(&provider),
                model: model.clone(),
            }),
            registry_factory: Box::new(DefaultToolRegistryFactory),
            provider,
            model,
            workspace_root,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_deps(
        name: impl Into<String>,
        description: impl Into<String>,
        provider: Arc<dyn LlmProvider>,
        model: impl Into<String>,
        workspace_root: PathBuf,
        turn_runner: Box<dyn TurnRunner>,
        registry_factory: Box<dyn ToolRegistryFactory>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            turn_runner,
            registry_factory,
            provider,
            model: model.into(),
            workspace_root,
        }
    }
}

impl Tool for AgentTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "The instruction or question for this sub-agent."
                    }
                },
                "required": ["prompt"],
                "additionalProperties": false
            }),
        }
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Agent { is_acp: false }
    }

    fn execute(&self, args: Value) -> ToolResult {
        let Some(prompt) = args.get("prompt").and_then(|v| v.as_str()) else {
            return ToolResult::failure("agent tool requires a string 'prompt' argument");
        };

        let ctx = ToolBuildContext {
            workspace_root: self.workspace_root.clone(),
            provider: Arc::clone(&self.provider),
            model: self.model.clone(),
        };
        let tools = match self.registry_factory.create(&ctx, ToolBuildProfile::SubAgent) {
            Ok(t) => t,
            Err(e) => return ToolResult::failure(format!("sub-agent tool assembly failed: {e}")),
        };

        let mut messages = vec![ProviderMessage::User {
            content: prompt.to_string(),
        }];

        let cancel_flag = Arc::new(AtomicBool::new(false));

        let outcome = self.turn_runner.run_turn(&mut messages, &tools, &cancel_flag);

        match outcome {
            TurnOutcome::Complete => {
                let text = messages
                    .iter()
                    .rev()
                    .find_map(|msg| match msg {
                        ProviderMessage::Assistant { content } => Some(content.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                ToolResult::success(text)
            }
            TurnOutcome::Cancelled => ToolResult::failure("sub-agent turn was cancelled"),
            TurnOutcome::TransientError(err) => {
                ToolResult::failure(format!("sub-agent error: {err}"))
            }
            TurnOutcome::FatalError { error, .. } => {
                ToolResult::failure(format!("sub-agent error: {error}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::runtime::RuntimeErrorKind;
    use crate::provider::echo::EchoProvider;

    /// A turn runner that appends an assistant message and returns a preset outcome.
    struct StubTurnRunner {
        outcome: TurnOutcome,
        assistant_text: Option<String>,
    }

    impl TurnRunner for StubTurnRunner {
        fn run_turn(
            &self,
            messages: &mut Vec<ProviderMessage>,
            _tools: &ToolRegistry,
            _cancel_flag: &Arc<AtomicBool>,
        ) -> TurnOutcome {
            if let Some(text) = &self.assistant_text {
                messages.push(ProviderMessage::Assistant {
                    content: text.clone(),
                });
            }
            match &self.outcome {
                TurnOutcome::Complete => TurnOutcome::Complete,
                TurnOutcome::Cancelled => TurnOutcome::Cancelled,
                TurnOutcome::TransientError(e) => TurnOutcome::TransientError(e.clone()),
                TurnOutcome::FatalError { kind, error } => TurnOutcome::FatalError {
                    kind: *kind,
                    error: error.clone(),
                },
            }
        }
    }

    /// A factory that returns an empty registry and asserts the expected profile.
    struct StubRegistryFactory;

    impl ToolRegistryFactory for StubRegistryFactory {
        fn create(
            &self,
            _ctx: &ToolBuildContext,
            profile: ToolBuildProfile,
        ) -> Result<ToolRegistry, String> {
            assert_eq!(
                profile,
                ToolBuildProfile::SubAgent,
                "AgentTool must request SubAgent profile"
            );
            Ok(ToolRegistry::new(PathBuf::from("/tmp/test-workspace")))
        }
    }

    /// A factory that always fails.
    struct FailingRegistryFactory;

    impl ToolRegistryFactory for FailingRegistryFactory {
        fn create(
            &self,
            _ctx: &ToolBuildContext,
            _profile: ToolBuildProfile,
        ) -> Result<ToolRegistry, String> {
            Err("simulated registry failure".to_string())
        }
    }

    fn make_agent(runner: StubTurnRunner, factory: impl ToolRegistryFactory + 'static) -> AgentTool {
        AgentTool::with_deps(
            "sub_agent",
            "test agent",
            Arc::new(EchoProvider::default()),
            "test-model",
            PathBuf::from("/tmp/test-workspace"),
            Box::new(runner),
            Box::new(factory),
        )
    }

    #[test]
    fn missing_prompt_returns_error() {
        let runner = StubTurnRunner {
            outcome: TurnOutcome::Complete,
            assistant_text: None,
        };
        let tool = make_agent(runner, StubRegistryFactory);
        let result = tool.execute(json!({}));
        assert!(result.is_error);
        assert!(result.text_output().contains("prompt"));
    }

    #[test]
    fn registry_failure_returns_error() {
        let runner = StubTurnRunner {
            outcome: TurnOutcome::Complete,
            assistant_text: None,
        };
        let tool = make_agent(runner, FailingRegistryFactory);
        let result = tool.execute(json!({"prompt": "do something"}));
        assert!(result.is_error);
        assert!(result.text_output().contains("sub-agent tool assembly failed"));
    }

    #[test]
    fn complete_with_assistant_text_returns_success() {
        let runner = StubTurnRunner {
            outcome: TurnOutcome::Complete,
            assistant_text: Some("done!".to_string()),
        };
        let tool = make_agent(runner, StubRegistryFactory);
        let result = tool.execute(json!({"prompt": "do something"}));
        assert!(!result.is_error);
        assert_eq!(result.text_output(), "done!");
    }

    #[test]
    fn complete_without_assistant_text_returns_empty() {
        let runner = StubTurnRunner {
            outcome: TurnOutcome::Complete,
            assistant_text: None,
        };
        let tool = make_agent(runner, StubRegistryFactory);
        let result = tool.execute(json!({"prompt": "do something"}));
        assert!(!result.is_error);
        assert_eq!(result.text_output(), "");
    }

    #[test]
    fn cancelled_outcome_returns_failure() {
        let runner = StubTurnRunner {
            outcome: TurnOutcome::Cancelled,
            assistant_text: None,
        };
        let tool = make_agent(runner, StubRegistryFactory);
        let result = tool.execute(json!({"prompt": "do something"}));
        assert!(result.is_error);
        assert!(result.text_output().contains("cancelled"));
    }

    #[test]
    fn transient_error_returns_failure() {
        let runner = StubTurnRunner {
            outcome: TurnOutcome::TransientError("timeout".to_string()),
            assistant_text: None,
        };
        let tool = make_agent(runner, StubRegistryFactory);
        let result = tool.execute(json!({"prompt": "do something"}));
        assert!(result.is_error);
        assert!(result.text_output().contains("timeout"));
    }

    #[test]
    fn fatal_error_returns_failure() {
        let runner = StubTurnRunner {
            outcome: TurnOutcome::FatalError {
                kind: RuntimeErrorKind::ToolExecution,
                error: "iteration limit".to_string(),
            },
            assistant_text: None,
        };
        let tool = make_agent(runner, StubRegistryFactory);
        let result = tool.execute(json!({"prompt": "do something"}));
        assert!(result.is_error);
        assert!(result.text_output().contains("iteration limit"));
    }
}
