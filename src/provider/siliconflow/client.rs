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

#[cfg(test)]
mod tests {
    mod http_test_server {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/common/http_test_server.rs"
        ));
    }

    use super::*;
    use crate::provider::ProviderEvent;
    use http_test_server::{http_response, spawn_one_shot_server};
    use std::net::TcpListener;
    use std::sync::mpsc;

    fn test_config(base_url: String) -> SiliconFlowConfig {
        SiliconFlowConfig {
            api_key: "test-key".into(),
            base_url,
            model: "Qwen/Qwen3-8B".into(),
            max_tokens: 128,
            temperature: 0.1,
            top_p: None,
            top_k: None,
            frequency_penalty: None,
            enable_thinking: None,
            thinking_budget: None,
            system_prompt: None,
            system_prompt_file: None,
        }
    }

    fn test_request() -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: "Qwen/Qwen3-8B".into(),
            messages: vec![crate::provider::siliconflow::ChatMessage::user("hi")],
            stream: None,
            max_tokens: Some(64),
            temperature: Some(0.2),
            top_p: None,
            top_k: None,
            frequency_penalty: None,
            enable_thinking: None,
            thinking_budget: None,
            stop: None,
            tools: None,
            tool_choice: None,
            n: None,
        }
    }

    #[test]
    fn auth_header_and_url_helpers_use_config() {
        let client = SiliconFlowClient::new(test_config("https://example.com/v1".into()));
        assert_eq!(client.auth_header(), "Bearer test-key");
        assert_eq!(client.url("/models"), "https://example.com/v1/models");
    }

    #[test]
    fn chat_sets_stream_false_and_parses_response() {
        let (base_url, handle) = spawn_one_shot_server(|request| {
            assert!(request.starts_with("POST /chat/completions HTTP/1.1"));
            assert!(request.contains("Authorization: Bearer test-key"));
            assert!(request.contains(r#""stream":false"#));
            http_response(
                200,
                "OK",
                "application/json",
                r#"{"id":"chat-1","choices":[{"index":0,"message":{"role":"assistant","content":"hello","tool_calls":null},"finish_reason":"stop"}],"usage":null,"model":"Qwen/Qwen3-8B"}"#,
            )
        });
        let client = SiliconFlowClient::new(test_config(base_url));
        let response = client.chat(&test_request()).expect("chat response");
        handle.join().expect("server joined");

        assert_eq!(response.id, "chat-1");
        assert_eq!(
            response.choices[0].message.content.as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn chat_returns_parse_error_with_response_body() {
        let (base_url, handle) =
            spawn_one_shot_server(|_| http_response(200, "OK", "application/json", "not-json"));
        let client = SiliconFlowClient::new(test_config(base_url));
        let err = client
            .chat(&test_request())
            .expect_err("parse error expected");
        handle.join().expect("server joined");

        assert!(err.contains("parse error"));
        assert!(err.contains("not-json"));
    }

    #[test]
    fn list_models_extracts_nested_error_message() {
        let (base_url, handle) = spawn_one_shot_server(|request| {
            assert!(request.starts_with("GET /models HTTP/1.1"));
            http_response(
                401,
                "Unauthorized",
                "application/json",
                r#"{"error":{"message":"invalid key"}}"#,
            )
        });
        let client = SiliconFlowClient::new(test_config(base_url));
        let err = client
            .list_models()
            .expect_err("list_models should surface API error");
        handle.join().expect("server joined");

        assert_eq!(err, "HTTP 401: invalid key");
    }

    #[test]
    fn embeddings_sends_payload_and_parses_success_response() {
        let (base_url, handle) = spawn_one_shot_server(|request| {
            assert!(request.starts_with("POST /embeddings HTTP/1.1"));
            assert!(request.contains(r#""model":"text-embedding""#));
            assert!(request.contains(r#""hello""#));
            http_response(
                200,
                "OK",
                "application/json",
                r#"{"data":[{"index":0,"embedding":[0.1,0.2]}],"model":"text-embedding","usage":{"prompt_tokens":1,"total_tokens":1}}"#,
            )
        });
        let client = SiliconFlowClient::new(test_config(base_url));
        let response = client
            .embeddings("text-embedding", EmbeddingInput::Single("hello".into()))
            .expect("embeddings should parse");
        handle.join().expect("server joined");

        assert_eq!(response.data.len(), 1);
        assert_eq!(response.model, "text-embedding");
    }

    #[test]
    fn chat_stream_emits_reasoning_text_and_tool_calls() {
        let sse = concat!(
            "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"think\"},\"finish_reason\":null}]}\n",
            "event: ping\n",
            "data: {not-json}\n",
            "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\",\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\\\"Ca\"}}]},\"finish_reason\":null}]}\n",
            "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"rgo.toml\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n",
            "data: [DONE]\n"
        );
        let (base_url, handle) =
            spawn_one_shot_server(move |_| http_response(200, "OK", "text/event-stream", sse));
        let client = SiliconFlowClient::new(test_config(base_url));
        let (tx, rx) = mpsc::channel();

        client
            .chat_stream(&test_request(), &tx)
            .expect("chat_stream should succeed");
        handle.join().expect("server joined");

        let events: Vec<_> = rx.try_iter().collect();
        assert!(events.contains(&ProviderEvent::ThinkingDelta("think".into())));
        assert!(events.contains(&ProviderEvent::TextDelta("hello".into())));
        assert!(events.contains(&ProviderEvent::ToolCall {
            call_id: "call-1".into(),
            name: "read_file".into(),
            arguments: "{\"path\":\"Cargo.toml\"}".into(),
        }));
        assert!(events.contains(&ProviderEvent::TurnComplete));
    }

    #[test]
    fn chat_stream_returns_ok_when_receiver_is_dropped() {
        let sse = concat!(
            "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n",
            "data: [DONE]\n"
        );
        let (base_url, handle) =
            spawn_one_shot_server(move |_| http_response(200, "OK", "text/event-stream", sse));
        let client = SiliconFlowClient::new(test_config(base_url));
        let (tx, rx) = mpsc::channel();
        drop(rx);

        let result = client.chat_stream(&test_request(), &tx);
        handle.join().expect("server joined");
        assert!(result.is_ok());
    }

    #[test]
    fn chat_stream_surfaces_status_message_from_top_level_message_field() {
        let (base_url, handle) = spawn_one_shot_server(|_| {
            http_response(
                429,
                "Too Many Requests",
                "application/json",
                r#"{"message":"rate limited"}"#,
            )
        });
        let client = SiliconFlowClient::new(test_config(base_url));
        let (tx, _rx) = mpsc::channel();
        let err = client
            .chat_stream(&test_request(), &tx)
            .expect_err("status error expected");
        handle.join().expect("server joined");

        assert_eq!(err, "HTTP 429: rate limited");
    }

    #[test]
    fn transport_errors_use_fallback_display_format() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        drop(listener);

        let client = SiliconFlowClient::new(test_config(format!("http://{addr}")));
        let err = client.list_models().expect_err("connection should fail");
        assert!(!err.starts_with("HTTP"));
    }
}
