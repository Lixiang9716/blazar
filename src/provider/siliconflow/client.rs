use std::io::BufRead;
use std::sync::mpsc::Sender;

use log::{debug, info, trace, warn};

use super::{
    ChatCompletionRequest, ChatCompletionResponse, EmbeddingInput, EmbeddingRequest,
    EmbeddingResponse, ModelsResponse, SiliconFlowConfig, StreamChunk, ToolCall,
    merge_tool_call_fragment,
};
use crate::provider::ProviderEvent;

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
