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
