use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::mpsc::Sender;

use async_openai::Client;
use async_openai::config::OpenAIConfig as AsyncOpenAiConfig;
use futures::StreamExt;
use log::{info, trace, warn};
use serde_json::{Map, Value, json};

use crate::agent::tools::ToolSpec;

use super::{LlmProvider, ProviderEvent, ProviderMessage};

// ── Configuration ──────────────────────────────────────────────────

/// Configuration for an OpenAI-compatible API provider.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OpenAiConfig {
    /// Provider backend: `"openai"` (default) or `"openrouter"`.
    #[serde(default)]
    pub provider_type: Option<String>,
    pub api_key: String,
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

fn default_model() -> String {
    "gpt-3.5-turbo".to_owned()
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

impl OpenAiConfig {
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
        cfg.resolve_system_prompt(repo_root);
        Ok(cfg)
    }

    /// If `system_prompt_file` is set, load the file content into `system_prompt`.
    pub fn resolve_system_prompt(&mut self, repo_root: &str) {
        if let Some(ref file_path) = self.system_prompt_file {
            let prompt_path = std::path::Path::new(repo_root).join(file_path);
            if let Ok(content) = std::fs::read_to_string(&prompt_path) {
                self.system_prompt = Some(content);
            }
        }
    }
}

// ── Byot stream response types ────────────────────────────────────
//
// These extend the standard OpenAI stream types with non-standard fields
// like `reasoning_content` used by Qwen3, DeepSeek-V3, etc.

#[derive(Debug, Clone, serde::Deserialize)]
pub struct StreamChunk {
    #[allow(dead_code)]
    pub id: String,
    #[serde(default)]
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct StreamChoice {
    #[allow(dead_code)]
    pub index: u32,
    pub delta: StreamDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct StreamDelta {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<DeltaToolCall>>,
    /// Chain-of-thought reasoning content (non-standard, Qwen3/DeepSeek).
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeltaToolCall {
    pub index: u32,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: Option<DeltaFunction>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeltaFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

// ── Tool call types (used by build_request and tests) ──────────────

/// A tool call accumulated from streamed fragments.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    None,
}

// ── LlmProvider implementation ─────────────────────────────────────

/// LLM provider backed by an OpenAI-compatible API via `async-openai`.
pub struct OpenAiProvider {
    config: OpenAiConfig,
    client: Client<AsyncOpenAiConfig>,
    runtime: tokio::runtime::Runtime,
}

impl OpenAiProvider {
    pub fn new(config: OpenAiConfig) -> Self {
        let async_config = AsyncOpenAiConfig::new()
            .with_api_key(&config.api_key)
            .with_api_base(&config.base_url);
        let client = Client::with_config(async_config);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create tokio runtime");
        Self {
            config,
            client,
            runtime,
        }
    }

    pub fn build_request_for_test(
        &self,
        messages: &[ProviderMessage],
        tools: &[ToolSpec],
    ) -> Value {
        self.build_request(&self.config.model, messages, tools)
    }

    /// Build a chat completion request as `serde_json::Value` (byot format).
    fn build_request(
        &self,
        model: &str,
        messages: &[ProviderMessage],
        tools: &[ToolSpec],
    ) -> Value {
        let cfg = &self.config;
        let truncated_messages = truncate_provider_messages(messages);
        let mut request_messages: Vec<Value> = Vec::new();

        if let Some(ref sys) = cfg.system_prompt {
            request_messages.push(json!({
                "role": "system",
                "content": render_system_prompt(sys)
            }));
        }

        let mut index = 0usize;
        while index < truncated_messages.len() {
            match &truncated_messages[index] {
                ProviderMessage::User { content } => {
                    request_messages.push(json!({
                        "role": "user",
                        "content": content
                    }));
                    index += 1;
                }
                ProviderMessage::Assistant { content } => {
                    let (tool_calls, next_index) =
                        collect_tool_call_batch(&truncated_messages, index + 1);
                    if tool_calls.is_empty() {
                        request_messages.push(json!({
                            "role": "assistant",
                            "content": content
                        }));
                    } else {
                        let tc_json = tool_calls_to_json(&tool_calls);
                        request_messages.push(json!({
                            "role": "assistant",
                            "content": content,
                            "tool_calls": tc_json
                        }));
                    }
                    index = next_index;
                }
                ProviderMessage::ToolCall { .. } => {
                    let (tool_calls, next_index) =
                        collect_tool_call_batch(&truncated_messages, index);
                    let tc_json = tool_calls_to_json(&tool_calls);
                    request_messages.push(json!({
                        "role": "assistant",
                        "tool_calls": tc_json
                    }));
                    index = next_index;
                }
                ProviderMessage::ToolResult {
                    tool_call_id,
                    output,
                    ..
                } => {
                    request_messages.push(json!({
                        "role": "tool",
                        "content": output,
                        "tool_call_id": tool_call_id
                    }));
                    index += 1;
                }
            }
        }

        let request_tools: Option<Vec<Value>> = if tools.is_empty() {
            None
        } else {
            Some(
                tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": tool.name,
                                "description": tool.description,
                                "parameters": tool.parameters
                            }
                        })
                    })
                    .collect(),
            )
        };
        let tool_choice = determine_tool_choice(&truncated_messages, request_tools.is_some());

        let mut obj = Map::new();
        obj.insert("model".into(), json!(model));
        obj.insert("messages".into(), json!(request_messages));
        obj.insert("stream".into(), json!(true));
        obj.insert("max_tokens".into(), json!(cfg.max_tokens));
        obj.insert("temperature".into(), json!(cfg.temperature));

        if let Some(top_p) = cfg.top_p {
            obj.insert("top_p".into(), json!(top_p));
        }
        if let Some(top_k) = cfg.top_k {
            obj.insert("top_k".into(), json!(top_k));
        }
        if let Some(fp) = cfg.frequency_penalty {
            obj.insert("frequency_penalty".into(), json!(fp));
        }
        if let Some(enable) = cfg.enable_thinking {
            obj.insert("enable_thinking".into(), json!(enable));
        }
        if let Some(budget) = cfg.thinking_budget {
            obj.insert("thinking_budget".into(), json!(budget));
        }
        if let Some(tools) = request_tools {
            obj.insert("tools".into(), json!(tools));
        }
        if let Some(tc) = tool_choice {
            obj.insert("tool_choice".into(), json!(tc));
        }

        Value::Object(obj)
    }
}

fn tool_calls_to_json(tool_calls: &[ToolCall]) -> Vec<Value> {
    tool_calls
        .iter()
        .map(|tc| {
            json!({
                "id": tc.id,
                "type": "function",
                "function": {
                    "name": tc.function.name,
                    "arguments": tc.function.arguments
                }
            })
        })
        .collect()
}

impl LlmProvider for OpenAiProvider {
    fn stream_turn(
        &self,
        model: &str,
        messages: &[ProviderMessage],
        tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        info!(
            "stream_turn: model={} messages={} tools={}",
            model,
            messages.len(),
            tools.len()
        );
        let req = self.build_request(model, messages, tools);

        let result = self.runtime.block_on(async {
            let mut stream = self
                .client
                .chat()
                .create_stream_byot::<Value, StreamChunk>(req)
                .await
                .map_err(|e| format!("stream error: {e}"))?;

            let mut tool_calls: Vec<ToolCall> = Vec::new();

            while let Some(chunk_result) = stream.next().await {
                let chunk: StreamChunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        trace!("stream: skip unparseable chunk: {e}");
                        continue;
                    }
                };

                for choice in &chunk.choices {
                    // Emit reasoning content (thinking mode)
                    if let Some(ref reasoning) = choice.delta.reasoning_content
                        && !reasoning.is_empty()
                        && tx
                            .send(ProviderEvent::ThinkingDelta(reasoning.clone()))
                            .is_err()
                    {
                        return Ok(());
                    }

                    // Emit regular content
                    if let Some(ref content) = choice.delta.content
                        && !content.is_empty()
                        && tx.send(ProviderEvent::TextDelta(content.clone())).is_err()
                    {
                        return Ok(());
                    }

                    // Accumulate streaming tool calls
                    if let Some(ref delta_tcs) = choice.delta.tool_calls {
                        for dtc in delta_tcs {
                            merge_tool_call_fragment(&mut tool_calls, dtc);
                        }
                    }

                    if let Some(ref reason) = choice.finish_reason
                        && reason == "tool_calls"
                        && !tool_calls.is_empty()
                    {
                        for tool_call in tool_calls.drain(..) {
                            let _ = tx.send(ProviderEvent::ToolCall {
                                call_id: tool_call.id,
                                name: tool_call.function.name,
                                arguments: tool_call.function.arguments,
                            });
                        }
                    }
                }
            }

            let _ = tx.send(ProviderEvent::TurnComplete);
            Ok::<(), String>(())
        });

        if let Err(e) = result {
            warn!("stream_turn: error={e}");
            let _ = tx.send(ProviderEvent::Error(e));
        }
    }

    fn list_models(&self) -> Result<Vec<super::ModelInfo>, String> {
        let resp = self.runtime.block_on(async {
            self.client
                .models()
                .list()
                .await
                .map_err(|e| format!("list_models error: {e}"))
        })?;
        let mut models: Vec<super::ModelInfo> = resp
            .data
            .into_iter()
            .map(|m| super::ModelInfo {
                description: m.id.clone(),
                id: m.id,
            })
            .collect();
        models.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(models)
    }
}

// ── Helpers ────────────────────────────────────────────────────────

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

pub(crate) fn render_system_prompt(base: &str) -> String {
    match build_runtime_context_block() {
        Some(context) => format!("{base}\n\n{context}"),
        None => base.to_owned(),
    }
}

fn build_runtime_context_block() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let platform = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    let git_branch = run_git_command(&cwd, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|| "unknown".to_owned());
    let git_status = run_git_command(
        &cwd,
        &["status", "--short", "--branch", "--untracked-files=no"],
    );

    let mut block = vec![
        "## Runtime Context".to_owned(),
        format!("- Working directory: {}", cwd.display()),
        format!("- Platform: {platform}"),
        format!("- Git branch: {git_branch}"),
    ];

    if let Some(status) = git_status
        && !status.is_empty()
    {
        block.push("- Git status:".to_owned());
        block.push("```text".to_owned());
        block.push(status);
        block.push("```".to_owned());
    }

    Some(block.join("\n"))
}

fn run_git_command(cwd: &std::path::Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn collect_tool_call_batch(messages: &[ProviderMessage], start: usize) -> (Vec<ToolCall>, usize) {
    let mut collected = Vec::new();
    let mut index = start;

    while index < messages.len() {
        match &messages[index] {
            ProviderMessage::ToolCall {
                id,
                name,
                arguments,
            } => {
                collected.push(ToolCall {
                    id: id.clone(),
                    call_type: "function".into(),
                    function: FunctionCall {
                        name: name.clone(),
                        arguments: arguments.clone(),
                    },
                });
                index += 1;
            }
            _ => break,
        }
    }

    (collected, index)
}

const MAX_CONTEXT_USER_TURNS: usize = 10;
const MAX_CONTEXT_PROVIDER_MESSAGES: usize = 80;

pub(crate) fn truncate_provider_messages(messages: &[ProviderMessage]) -> Vec<ProviderMessage> {
    if messages.is_empty() {
        return Vec::new();
    }

    let user_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| match message {
            ProviderMessage::User { .. } => Some(index),
            _ => None,
        })
        .collect();

    let mut start = 0usize;
    if user_indices.len() > MAX_CONTEXT_USER_TURNS {
        start = user_indices[user_indices.len() - MAX_CONTEXT_USER_TURNS];
    }

    if messages.len().saturating_sub(start) > MAX_CONTEXT_PROVIDER_MESSAGES {
        let tail_start = messages.len() - MAX_CONTEXT_PROVIDER_MESSAGES;
        start = user_indices
            .iter()
            .copied()
            .find(|idx| *idx >= tail_start)
            .unwrap_or(tail_start)
            .max(start);
    }

    messages[start..].to_vec()
}

pub(crate) fn determine_tool_choice(
    messages: &[ProviderMessage],
    has_tools: bool,
) -> Option<ToolChoice> {
    if !has_tools {
        return None;
    }
    if has_repeated_successful_tool_calls(messages) {
        Some(ToolChoice::None)
    } else {
        Some(ToolChoice::Auto)
    }
}

fn has_repeated_successful_tool_calls(messages: &[ProviderMessage]) -> bool {
    let mut pending_calls: HashMap<&str, (&str, &str)> = HashMap::new();
    let mut seen_successes: HashSet<(String, String, String)> = HashSet::new();

    for message in messages {
        match message {
            ProviderMessage::ToolCall {
                id,
                name,
                arguments,
            } => {
                pending_calls.insert(id.as_str(), (name.as_str(), arguments.as_str()));
            }
            ProviderMessage::ToolResult {
                tool_call_id,
                output,
                is_error: false,
            } => {
                if let Some((name, arguments)) = pending_calls.remove(tool_call_id.as_str()) {
                    let success = (name.to_string(), arguments.to_string(), output.clone());
                    if !seen_successes.insert(success) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }

    false
}
