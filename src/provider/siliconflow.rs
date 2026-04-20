use std::io::BufRead;
use std::sync::mpsc::Sender;

use log::{debug, info, trace, warn};

use super::{LlmProvider, ProviderEvent};

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
    #[serde(default)]
    pub enable_thinking: Option<bool>,
    /// Max tokens for chain-of-thought output (128..=32768).
    #[serde(default)]
    pub thinking_budget: Option<u32>,
    /// System prompt prepended to every conversation.
    #[serde(default)]
    pub system_prompt: Option<String>,
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

impl SiliconFlowConfig {
    /// Load from `config/provider.json` relative to `repo_root`.
    pub fn load(repo_root: &str) -> Result<Self, String> {
        let path = std::path::Path::new(repo_root).join("config/provider.json");
        let data = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        serde_json::from_str(&data).map_err(|e| format!("invalid provider config: {e}"))
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
    pub n: Option<u32>,
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

    // ── Chat Completions (non-streaming) ───────────────────────────

    /// Send a non-streaming chat completion request.
    pub fn chat(&self, req: &ChatCompletionRequest) -> Result<ChatCompletionResponse, String> {
        let mut request = req.clone();
        request.stream = Some(false);

        let resp: ureq::Response = ureq::post(&self.url("/chat/completions"))
            .set("Authorization", &self.auth_header())
            .send_json(serde_json::to_value(&request).map_err(|e| e.to_string())?)
            .map_err(|e| format!("HTTP error: {e}"))?;

        let body: String = resp.into_string().map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| format!("parse error: {e}\nbody: {body}"))
    }

    // ── Chat Completions (streaming via channel) ───────────────────

    /// Stream chat completion chunks into the provided channel.
    ///
    /// Sends `ProviderEvent::TextDelta` for content and reasoning,
    /// `ProviderEvent::ToolCallRequest` when the model requests tool calls,
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
                warn!("chat_stream: HTTP error: {e}");
                format!("HTTP error: {e}")
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
                        if let Some(ref f) = dtc.function {
                            if let Some(ref name) = f.name {
                                tool_calls[idx].function.name.clone_from(name);
                            }
                            if let Some(ref args) = f.arguments {
                                tool_calls[idx].function.arguments.push_str(args);
                            }
                        }
                    }
                }

                if let Some(ref reason) = choice.finish_reason
                    && reason == "tool_calls"
                    && !tool_calls.is_empty()
                {
                    let json = serde_json::to_string(&tool_calls).unwrap_or_default();
                    let _ = tx.send(ProviderEvent::ToolCallRequest(json));
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
            .map_err(|e| format!("HTTP error: {e}"))?;

        let body: String = resp.into_string().map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| format!("parse error: {e}"))
    }

    // ── Models ─────────────────────────────────────────────────────

    /// List available models.
    pub fn list_models(&self) -> Result<ModelsResponse, String> {
        let resp: ureq::Response = ureq::get(&self.url("/models"))
            .set("Authorization", &self.auth_header())
            .call()
            .map_err(|e| format!("HTTP error: {e}"))?;

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

    /// Build a `ChatCompletionRequest` from a user prompt, applying config defaults.
    fn build_request(&self, prompt: &str) -> ChatCompletionRequest {
        let cfg = self.client.config();
        let mut messages = Vec::new();

        if let Some(ref sys) = cfg.system_prompt {
            messages.push(ChatMessage::system(sys.clone()));
        }
        messages.push(ChatMessage::user(prompt));

        ChatCompletionRequest {
            model: cfg.model.clone(),
            messages,
            stream: Some(true),
            max_tokens: Some(cfg.max_tokens),
            temperature: Some(cfg.temperature),
            top_p: cfg.top_p,
            top_k: cfg.top_k,
            frequency_penalty: cfg.frequency_penalty,
            enable_thinking: cfg.enable_thinking,
            thinking_budget: cfg.thinking_budget,
            stop: None,
            tools: None,
            n: None,
        }
    }
}

impl LlmProvider for SiliconFlowProvider {
    fn stream_turn(&self, prompt: &str, tx: Sender<ProviderEvent>) {
        info!("stream_turn: prompt_len={}", prompt.len());
        let req = self.build_request(prompt);
        if let Err(e) = self.client.chat_stream(&req, &tx) {
            warn!("stream_turn: error={e}");
            let _ = tx.send(ProviderEvent::Error(e));
        }
    }
}
