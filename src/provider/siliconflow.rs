use std::sync::mpsc::Sender;

use log::{info, warn};

use crate::agent::tools::ToolSpec;

use super::{LlmProvider, ProviderEvent, ProviderMessage};

mod client;
mod request_builder;
mod types;

pub use client::SiliconFlowClient;
pub use types::*;

use request_builder::{
    collect_tool_call_batch, determine_tool_choice, render_system_prompt,
    truncate_provider_messages,
};

// ── Configuration ──────────────────────────────────────────────────

/// Configuration for the SiliconFlow provider.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SiliconFlowConfig {
    pub api_key: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub top_k: Option<f32>,
    #[serde(default)]
    pub frequency_penalty: Option<f32>,
    /// Enable chain-of-thought (for supported models like Qwen3, DeepSeek-V3).
    #[serde(default = "default_enable_thinking")]
    pub enable_thinking: Option<bool>,
    /// Max tokens for chain-of-thought output (128..=32768).
    #[serde(default)]
    pub thinking_budget: Option<u32>,
    /// Inline system prompt prepended to every conversation.
    /// If both `system_prompt` and `system_prompt_file` are set, the file wins.
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Path to a markdown file containing the system prompt (relative to repo root).
    #[serde(default)]
    pub system_prompt_file: Option<String>,
}

fn default_base_url() -> String {
    "https://api.siliconflow.cn/v1".to_owned()
}
fn default_model() -> String {
    "Qwen/Qwen3-8B".to_owned()
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_temperature() -> f32 {
    0.7
}
fn default_enable_thinking() -> Option<bool> {
    Some(false)
}

/// Curated list of popular SiliconFlow models suitable for tool-calling agents.
pub const POPULAR_MODELS: &[(&str, &str)] = &[
    ("Qwen/Qwen3-8B", "Qwen3 8B — fast, free tier"),
    ("Qwen/Qwen3-32B", "Qwen3 32B — balanced"),
    ("Qwen/Qwen3-235B-A22B", "Qwen3 235B MoE — strongest"),
    ("Qwen/Qwen2.5-72B-Instruct", "Qwen2.5 72B Instruct"),
    ("deepseek-ai/DeepSeek-V3", "DeepSeek V3"),
    ("deepseek-ai/DeepSeek-R1", "DeepSeek R1 — reasoning"),
    ("Pro/deepseek-ai/DeepSeek-V3", "DeepSeek V3 Pro — optimised"),
];

impl SiliconFlowConfig {
    /// Load from `config/provider.json` relative to `repo_root`.
    ///
    /// If `system_prompt_file` is set, the file content replaces `system_prompt`.
    pub fn load(repo_root: &str) -> Result<Self, String> {
        let root = std::path::Path::new(repo_root);
        let path = root.join("config/provider.json");
        let data = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let mut cfg: Self =
            serde_json::from_str(&data).map_err(|e| format!("invalid provider config: {e}"))?;

        // Load system prompt from file if specified.
        if let Some(ref file_path) = cfg.system_prompt_file {
            let prompt_path = root.join(file_path);
            let content = std::fs::read_to_string(&prompt_path).map_err(|e| {
                format!(
                    "cannot read system_prompt_file {}: {e}",
                    prompt_path.display()
                )
            })?;
            cfg.system_prompt = Some(content);
        }

        Ok(cfg)
    }
}

/// LLM provider backed by the SiliconFlow API.
pub struct SiliconFlowProvider {
    client: SiliconFlowClient,
}

impl SiliconFlowProvider {
    pub fn new(config: SiliconFlowConfig) -> Self {
        Self {
            client: SiliconFlowClient::new(config),
        }
    }

    /// Access the underlying client for non-chat APIs (embeddings, models).
    pub fn client(&self) -> &SiliconFlowClient {
        &self.client
    }

    pub fn build_request_for_test(
        &self,
        messages: &[ProviderMessage],
        tools: &[ToolSpec],
    ) -> ChatCompletionRequest {
        self.build_request(messages, tools)
    }

    /// Build a `ChatCompletionRequest` from message history, applying config defaults.
    fn build_request(
        &self,
        messages: &[ProviderMessage],
        tools: &[ToolSpec],
    ) -> ChatCompletionRequest {
        let cfg = self.client.config();
        let truncated_messages = truncate_provider_messages(messages);
        let mut request_messages = Vec::new();

        if let Some(ref sys) = cfg.system_prompt {
            request_messages.push(ChatMessage::system(render_system_prompt(sys)));
        }

        let mut index = 0usize;
        while index < truncated_messages.len() {
            match &truncated_messages[index] {
                ProviderMessage::User { content } => {
                    request_messages.push(ChatMessage::user(content.clone()));
                    index += 1;
                }
                ProviderMessage::Assistant { content } => {
                    let (tool_calls, next_index) =
                        collect_tool_call_batch(&truncated_messages, index + 1);
                    if tool_calls.is_empty() {
                        request_messages.push(ChatMessage::assistant(content.clone()));
                    } else {
                        request_messages.push(ChatMessage {
                            role: Role::Assistant,
                            content: Some(content.clone()),
                            tool_calls: Some(tool_calls),
                            tool_call_id: None,
                        });
                    }
                    index = next_index;
                }
                ProviderMessage::ToolCall { .. } => {
                    let (tool_calls, next_index) =
                        collect_tool_call_batch(&truncated_messages, index);
                    request_messages.push(ChatMessage {
                        role: Role::Assistant,
                        content: None,
                        tool_calls: Some(tool_calls),
                        tool_call_id: None,
                    });
                    index = next_index;
                }
                ProviderMessage::ToolResult {
                    tool_call_id,
                    output,
                    ..
                } => {
                    request_messages.push(ChatMessage::tool_result(
                        tool_call_id.clone(),
                        output.clone(),
                    ));
                    index += 1;
                }
            }
        }

        let request_tools = if tools.is_empty() {
            None
        } else {
            Some(
                tools
                    .iter()
                    .map(|tool| Tool {
                        tool_type: "function".into(),
                        function: FunctionDef {
                            name: tool.name.clone(),
                            description: tool.description.clone(),
                            parameters: tool.parameters.clone(),
                        },
                    })
                    .collect(),
            )
        };
        let tool_choice = determine_tool_choice(&truncated_messages, request_tools.is_some());

        ChatCompletionRequest {
            model: cfg.model.clone(),
            messages: request_messages,
            stream: Some(true),
            max_tokens: Some(cfg.max_tokens),
            temperature: Some(cfg.temperature),
            top_p: cfg.top_p,
            top_k: cfg.top_k,
            frequency_penalty: cfg.frequency_penalty,
            enable_thinking: cfg.enable_thinking,
            thinking_budget: cfg.thinking_budget,
            stop: None,
            tools: request_tools,
            tool_choice,
            n: None,
        }
    }
}

impl LlmProvider for SiliconFlowProvider {
    fn stream_turn(
        &self,
        messages: &[ProviderMessage],
        tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        info!(
            "stream_turn: messages={} tools={}",
            messages.len(),
            tools.len()
        );
        let req = self.build_request(messages, tools);
        if let Err(e) = self.client.chat_stream(&req, &tx) {
            warn!("stream_turn: error={e}");
            let _ = tx.send(ProviderEvent::Error(e));
        }
    }
}

pub fn merge_tool_call_fragment(tool_calls: &mut Vec<ToolCall>, dtc: &DeltaToolCall) {
    let idx = dtc.index as usize;
    while tool_calls.len() <= idx {
        tool_calls.push(ToolCall {
            id: String::new(),
            call_type: "function".to_owned(),
            function: FunctionCall {
                name: String::new(),
                arguments: String::new(),
            },
        });
    }

    if let Some(ref id) = dtc.id {
        tool_calls[idx].id.clone_from(id);
    }
    if let Some(ref call_type) = dtc.call_type {
        tool_calls[idx].call_type.clone_from(call_type);
    }
    if let Some(ref function) = dtc.function {
        if let Some(ref name) = function.name {
            tool_calls[idx].function.name.clone_from(name);
        }
        if let Some(ref arguments) = function.arguments {
            tool_calls[idx].function.arguments.push_str(arguments);
        }
    }
}
