use std::collections::{HashMap, HashSet};
use std::io::BufRead;
use std::process::Command;
use std::sync::mpsc::Sender;

use log::{debug, info, trace, warn};

use crate::agent::tools::ToolSpec;

use super::{LlmProvider, ProviderEvent, ProviderMessage};

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

// ── Chat Completions types ─────────────────────────────────────────

/// Role in a chat conversation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A single message in the conversation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool calls requested by the assistant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// ID of the tool call this message responds to (role=tool).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

// ── Tool / Function Calling types ──────────────────────────────────

/// A tool the model may call.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

/// Function definition for tool calling.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A tool call returned by the model.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

/// The function name + arguments the model wants to invoke.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// JSON-encoded arguments string.
    pub arguments: String,
}

// ── Chat Completions request / response ────────────────────────────

/// Request body for `/v1/chat/completions`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    None,
}

/// Non-streaming response from `/v1/chat/completions`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
    pub model: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChoiceMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChoiceMessage {
    pub role: Role,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// A single SSE chunk during streaming.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct StreamChunk {
    pub id: String,
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct StreamChoice {
    pub index: u32,
    pub delta: Delta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Delta {
    pub role: Option<Role>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<DeltaToolCall>>,
    /// Chain-of-thought reasoning content (when thinking is enabled).
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

// ── Embeddings types ───────────────────────────────────────────────

/// Request body for `/v1/embeddings`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: EmbeddingInput,
}

/// Input can be a single string or a batch of strings.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Batch(Vec<String>),
}

/// Response from `/v1/embeddings`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EmbeddingResponse {
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: EmbeddingUsage,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct EmbeddingData {
    pub index: u32,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct EmbeddingUsage {
    pub prompt_tokens: u32,
    pub total_tokens: u32,
}

// ── Models types ───────────────────────────────────────────────────

/// A model entry returned by `/v1/models`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub object: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModelsResponse {
    pub data: Vec<ModelInfo>,
}

// ── SiliconFlow Client ─────────────────────────────────────────────

/// Blocking HTTP client for the SiliconFlow API.
///
/// Wraps `ureq` and handles authentication, serialization, and SSE parsing.
/// Designed to run on background threads (no async).
pub struct SiliconFlowClient {
    config: SiliconFlowConfig,
}

impl SiliconFlowClient {
    pub fn new(config: SiliconFlowConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &SiliconFlowConfig {
        &self.config
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.config.api_key)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.config.base_url)
    }

    /// Convert a ureq error into a descriptive string, extracting the
    /// API error message from the response body when available.
    fn describe_error(e: ureq::Error) -> String {
        match e {
            ureq::Error::Status(code, resp) => {
                let body = resp.into_string().unwrap_or_default();
                let msg = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|v| {
                        v.get("message")
                            .or_else(|| v.get("error").and_then(|err| err.get("message")))
                            .and_then(|m| m.as_str().map(String::from))
                    })
                    .unwrap_or(body);
                format!("HTTP {code}: {msg}")
            }
            other => format!("{other}"),
        }
    }

    // ── Chat Completions (non-streaming) ───────────────────────────

    /// Send a non-streaming chat completion request.
    pub fn chat(&self, req: &ChatCompletionRequest) -> Result<ChatCompletionResponse, String> {
        let mut request = req.clone();
        request.stream = Some(false);

        let resp: ureq::Response = ureq::post(&self.url("/chat/completions"))
            .set("Authorization", &self.auth_header())
            .send_json(serde_json::to_value(&request).map_err(|e| e.to_string())?)
            .map_err(Self::describe_error)?;

        let body: String = resp.into_string().map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| format!("parse error: {e}\nbody: {body}"))
    }

    // ── Chat Completions (streaming via channel) ───────────────────

    /// Stream chat completion chunks into the provided channel.
    ///
    /// Sends `ProviderEvent::TextDelta` for content and reasoning,
    /// `ProviderEvent::ToolCall` when the model requests tool calls,
    /// and `ProviderEvent::TurnComplete` or `ProviderEvent::Error` at the end.
    pub fn chat_stream(
        &self,
        req: &ChatCompletionRequest,
        tx: &Sender<ProviderEvent>,
    ) -> Result<(), String> {
        let mut request = req.clone();
        request.stream = Some(true);

        info!(
            "chat_stream: model={} messages={}",
            request.model,
            request.messages.len()
        );

        let resp: ureq::Response = ureq::post(&self.url("/chat/completions"))
            .set("Authorization", &self.auth_header())
            .send_json(serde_json::to_value(&request).map_err(|e| e.to_string())?)
            .map_err(|e| {
                let detail = Self::describe_error(e);
                warn!("chat_stream: API error: {detail}");
                detail
            })?;

        debug!("chat_stream: response received, streaming SSE");
        let reader = resp.into_reader();
        let buf = std::io::BufReader::new(reader);

        // Accumulate tool call fragments across chunks.
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        for line_result in buf.lines() {
            let line: String = line_result.map_err(|e| format!("stream read error: {e}"))?;

            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data.trim() == "[DONE]" {
                break;
            }

            let chunk: StreamChunk = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(e) => {
                    trace!("chat_stream: skip unparseable chunk: {e}");
                    continue;
                }
            };

            for choice in &chunk.choices {
                // Emit reasoning content (thinking mode) as a separate event
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
        Ok(())
    }

    // ── Embeddings ─────────────────────────────────────────────────

    /// Create embeddings for the given input.
    pub fn embeddings(
        &self,
        model: &str,
        input: EmbeddingInput,
    ) -> Result<EmbeddingResponse, String> {
        let req = EmbeddingRequest {
            model: model.to_owned(),
            input,
        };

        let resp: ureq::Response = ureq::post(&self.url("/embeddings"))
            .set("Authorization", &self.auth_header())
            .send_json(serde_json::to_value(&req).map_err(|e| e.to_string())?)
            .map_err(Self::describe_error)?;

        let body: String = resp.into_string().map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| format!("parse error: {e}"))
    }

    // ── Models ─────────────────────────────────────────────────────

    /// List available models.
    pub fn list_models(&self) -> Result<ModelsResponse, String> {
        let resp: ureq::Response = ureq::get(&self.url("/models"))
            .set("Authorization", &self.auth_header())
            .call()
            .map_err(Self::describe_error)?;

        let body: String = resp.into_string().map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| format!("parse error: {e}"))
    }
}

// ── LlmProvider implementation ─────────────────────────────────────

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

fn render_system_prompt(base: &str) -> String {
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

fn truncate_provider_messages(messages: &[ProviderMessage]) -> Vec<ProviderMessage> {
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

fn determine_tool_choice(messages: &[ProviderMessage], has_tools: bool) -> Option<ToolChoice> {
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
