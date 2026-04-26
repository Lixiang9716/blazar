use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use super::bash::BashTool;
use super::list_dir::ListDirTool;
use super::read_file::ReadFileTool;
use super::write_file::WriteFileTool;
use super::{
    BuiltinToolDescriptor, BuiltinToolProfiles, Tool, ToolBuildContext, ToolKind, ToolRegistry,
    ToolResult, ToolSpec,
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
}

impl AgentTool {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        provider: Arc<dyn LlmProvider>,
        model: impl Into<String>,
        workspace_root: PathBuf,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
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

        let mut tools = ToolRegistry::new(self.workspace_root.clone());
        tools.register(Box::new(ReadFileTool::new(self.workspace_root.clone())));
        tools.register(Box::new(WriteFileTool::new(self.workspace_root.clone())));
        tools.register(Box::new(ListDirTool::new(self.workspace_root.clone())));
        tools.register(Box::new(BashTool::new(self.workspace_root.clone())));

        let mut messages = vec![ProviderMessage::User {
            content: prompt.to_string(),
        }];

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let observer = SilentObserver;

        let outcome = execute_turn(
            &mut messages,
            &*self.provider,
            &self.model,
            &tools,
            &observer,
            &cancel_flag,
        );

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
