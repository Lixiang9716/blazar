use std::sync::mpsc::Sender;

use futures::StreamExt;
use log::{info, warn};

use openrouter_rs::OpenRouterClient;
use openrouter_rs::api::chat::{ChatCompletionRequest, Message as OrMessage};
use openrouter_rs::types::Role as OrRole;
use openrouter_rs::types::stream::StreamEvent;
use openrouter_rs::types::tool::Tool as OrTool;

use crate::agent::tools::ToolSpec;

use super::openai_compat::{
    OpenAiConfig, ToolChoice, determine_tool_choice, render_system_prompt,
    truncate_provider_messages,
};
use super::{LlmProvider, ModelInfo, ProviderEvent, ProviderMessage, ProviderUsage};

// ── Provider ───────────────────────────────────────────────────────

pub struct OpenRouterProvider {
    config: OpenAiConfig,
    client: OpenRouterClient,
    runtime: tokio::runtime::Runtime,
}

impl OpenRouterProvider {
    pub fn new(config: OpenAiConfig) -> Self {
        let client = OpenRouterClient::builder()
            .api_key(&config.api_key)
            .http_referer("https://github.com/Lixiang9716/blazar")
            .x_title("Blazar")
            .build()
            .expect("failed to build OpenRouterClient");

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
}

fn context_length_to_u32(value: f64) -> Option<u32> {
    if value.is_finite() && value >= 0.0 && value <= u32::MAX as f64 {
        Some(value as u32)
    } else {
        None
    }
}

fn map_openrouter_model_info(model: openrouter_rs::api::models::Model) -> ModelInfo {
    ModelInfo {
        description: model.name.clone(),
        id: model.id,
        context_length: context_length_to_u32(model.context_length),
    }
}

fn handle_stream_event(tx: &Sender<ProviderEvent>, event: StreamEvent) -> bool {
    match event {
        StreamEvent::ContentDelta(text) => tx.send(ProviderEvent::TextDelta(text)).is_ok(),
        StreamEvent::ReasoningDelta(text) => tx.send(ProviderEvent::ThinkingDelta(text)).is_ok(),
        StreamEvent::ReasoningDetailsDelta(_) => true,
        StreamEvent::Done {
            tool_calls, usage, ..
        } => {
            if let Some(usage) = usage {
                let _ = tx.send(ProviderEvent::Usage(ProviderUsage {
                    prompt_tokens: usage.prompt_tokens,
                    completion_tokens: usage.completion_tokens,
                    total_tokens: usage.total_tokens,
                }));
            }
            for tc in tool_calls {
                let _ = tx.send(ProviderEvent::ToolCall {
                    call_id: tc.id().to_owned(),
                    name: tc.name().to_owned(),
                    arguments: tc.arguments_json().to_owned(),
                });
            }
            let _ = tx.send(ProviderEvent::TurnComplete);
            false
        }
        StreamEvent::Error(error) => {
            let _ = tx.send(ProviderEvent::Error(format!("{error}")));
            false
        }
    }
}

// ── LlmProvider trait ──────────────────────────────────────────────

impl LlmProvider for OpenRouterProvider {
    fn stream_turn(
        &self,
        model: &str,
        messages: &[ProviderMessage],
        tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        info!(
            "openrouter stream_turn: model={} messages={} tools={}",
            model,
            messages.len(),
            tools.len()
        );
        let request = match build_chat_request(&self.config, model, messages, tools) {
            Ok(request) => request,
            Err(error) => {
                let _ = tx.send(ProviderEvent::Error(error));
                return;
            }
        };

        // Stream via tokio block_on
        let result = self.runtime.block_on(async {
            let mut stream = self
                .client
                .chat()
                .stream_tool_aware(&request)
                .await
                .map_err(|e| format!("stream error: {e}"))?;

            while let Some(event) = stream.next().await {
                if !handle_stream_event(&tx, event) {
                    return Ok(());
                }
            }

            // Stream ended without Done event
            let _ = tx.send(ProviderEvent::TurnComplete);
            Ok::<(), String>(())
        });

        if let Err(e) = result {
            warn!("openrouter stream_turn: error={e}");
            let _ = tx.send(ProviderEvent::Error(e));
        }
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        let models = self.runtime.block_on(async {
            self.client
                .models()
                .list()
                .await
                .map_err(|e| format!("list_models error: {e}"))
        })?;

        let mut result: Vec<ModelInfo> =
            models.into_iter().map(map_openrouter_model_info).collect();
        result.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(result)
    }
}

// ── Helpers ────────────────────────────────────────────────────────

/// Collect consecutive `ProviderMessage::ToolCall` entries starting at `start`
/// and convert them to openrouter-rs `ToolCall` objects for the assistant
/// message history.
fn collect_tool_calls(
    messages: &[ProviderMessage],
    start: usize,
) -> (Vec<openrouter_rs::types::ToolCall>, usize) {
    use openrouter_rs::types::completion::FunctionCall as OrFunctionCall;

    let mut collected = Vec::new();
    let mut index = start;

    while index < messages.len() {
        match &messages[index] {
            ProviderMessage::ToolCall {
                id,
                name,
                arguments,
            } => {
                collected.push(openrouter_rs::types::ToolCall {
                    id: id.clone(),
                    type_: "function".into(),
                    function: OrFunctionCall {
                        name: name.clone(),
                        arguments: arguments.clone(),
                    },
                    index: None,
                });
                index += 1;
            }
            _ => break,
        }
    }

    (collected, index)
}

fn build_chat_request(
    config: &OpenAiConfig,
    model: &str,
    messages: &[ProviderMessage],
    tools: &[ToolSpec],
) -> Result<ChatCompletionRequest, String> {
    let truncated = truncate_provider_messages(messages);
    let or_messages = build_or_messages(config, &truncated);
    let or_tools: Vec<OrTool> = tools
        .iter()
        .map(|tool| OrTool::new(&tool.name, &tool.description, tool.parameters.clone()))
        .collect();

    let mut builder = ChatCompletionRequest::builder();
    builder
        .model(model)
        .messages(or_messages)
        .max_tokens(config.max_tokens)
        .temperature(f64::from(config.temperature));

    if let Some(top_p) = config.top_p {
        builder.top_p(f64::from(top_p));
    }
    if let Some(fp) = config.frequency_penalty {
        builder.frequency_penalty(f64::from(fp));
    }

    if !or_tools.is_empty() {
        builder.tools(or_tools);
        match determine_tool_choice(&truncated, true) {
            Some(ToolChoice::Auto) => {
                builder.tool_choice_auto();
            }
            Some(ToolChoice::None) => {
                builder.tool_choice_none();
            }
            None => {}
        }
    }

    if config.enable_thinking == Some(true) {
        builder.enable_reasoning();
    }

    builder
        .build()
        .map_err(|error| format!("request build error: {error}"))
}

fn build_or_messages(config: &OpenAiConfig, messages: &[ProviderMessage]) -> Vec<OrMessage> {
    let mut or_messages: Vec<OrMessage> = Vec::new();

    if let Some(ref prompt) = config.system_prompt {
        or_messages.push(OrMessage::new(OrRole::System, render_system_prompt(prompt)));
    }

    let mut idx = 0usize;
    while idx < messages.len() {
        match &messages[idx] {
            ProviderMessage::User { content } => {
                or_messages.push(OrMessage::new(OrRole::User, content.as_str()));
                idx += 1;
            }
            ProviderMessage::Assistant { content } => {
                let (tool_calls, next_idx) = collect_tool_calls(messages, idx + 1);
                if tool_calls.is_empty() {
                    or_messages.push(OrMessage::new(OrRole::Assistant, content.as_str()));
                } else {
                    or_messages.push(OrMessage::assistant_with_tool_calls(
                        content.as_str(),
                        tool_calls,
                    ));
                }
                idx = next_idx;
            }
            ProviderMessage::ToolCall { .. } => {
                let (tool_calls, next_idx) = collect_tool_calls(messages, idx);
                or_messages.push(OrMessage::assistant_with_tool_calls("", tool_calls));
                idx = next_idx;
            }
            ProviderMessage::ToolResult {
                tool_call_id,
                output,
                ..
            } => {
                or_messages.push(OrMessage::tool_response(tool_call_id, output.as_str()));
                idx += 1;
            }
        }
    }

    or_messages
}

#[cfg(test)]
mod tests {
    use super::{
        build_chat_request, build_or_messages, collect_tool_calls, handle_stream_event,
        map_openrouter_model_info,
    };
    use crate::agent::tools::ToolSpec;
    use crate::provider::openai_compat::OpenAiConfig;
    use crate::provider::{ProviderEvent, ProviderMessage, ProviderUsage};
    use openrouter_rs::api::models::{Architecture, Model, Pricing, TopProvider};
    use openrouter_rs::types::completion::ResponseUsage;
    use openrouter_rs::types::stream::StreamEvent;
    use serde_json::json;

    fn test_config() -> OpenAiConfig {
        OpenAiConfig {
            provider_type: Some("openrouter".to_owned()),
            api_key: "test-key".to_owned(),
            base_url: "https://openrouter.ai/api/v1".to_owned(),
            model: "openrouter/auto".to_owned(),
            max_tokens: 512,
            temperature: 0.1,
            top_p: Some(0.8),
            top_k: None,
            frequency_penalty: Some(0.2),
            enable_thinking: Some(true),
            thinking_budget: None,
            system_prompt: Some("Follow instructions".to_owned()),
            system_prompt_file: None,
        }
    }

    #[test]
    fn collect_tool_calls_collects_consecutive_tool_calls() {
        let messages = vec![
            ProviderMessage::Assistant {
                content: "planning".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-1".into(),
                name: "read_file".into(),
                arguments: "{\"path\":\"a.txt\"}".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-2".into(),
                name: "list_dir".into(),
                arguments: "{\"path\":\".\"}".into(),
            },
            ProviderMessage::User {
                content: "continue".into(),
            },
        ];

        let (calls, next) = collect_tool_calls(&messages, 1);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call-1");
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(calls[1].id, "call-2");
        assert_eq!(calls[1].function.name, "list_dir");
        assert_eq!(next, 3);
    }

    #[test]
    fn collect_tool_calls_stops_when_non_tool_message_appears() {
        let messages = vec![
            ProviderMessage::ToolCall {
                id: "call-1".into(),
                name: "read_file".into(),
                arguments: "{}".into(),
            },
            ProviderMessage::Assistant {
                content: "done".into(),
            },
        ];

        let (calls, next) = collect_tool_calls(&messages, 0);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call-1");
        assert_eq!(next, 1);
    }

    #[test]
    fn build_or_messages_covers_orphan_tool_calls_and_tool_results() {
        let cfg = test_config();
        let messages = vec![
            ProviderMessage::ToolCall {
                id: "call-orphan".into(),
                name: "read_file".into(),
                arguments: "{\"path\":\"Cargo.toml\"}".into(),
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-orphan".into(),
                output: "content".into(),
                is_error: false,
            },
        ];

        let converted = build_or_messages(&cfg, &messages);
        assert_eq!(converted.len(), 3);
    }

    #[test]
    fn build_chat_request_accepts_messages_with_tools_and_reasoning() {
        let cfg = test_config();
        let messages = vec![
            ProviderMessage::User {
                content: "inspect files".into(),
            },
            ProviderMessage::Assistant {
                content: "I will call tools.".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-1".into(),
                name: "read_file".into(),
                arguments: "{\"path\":\"Cargo.toml\"}".into(),
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-1".into(),
                output: "ok".into(),
                is_error: false,
            },
        ];
        let tools = vec![ToolSpec {
            name: "read_file".into(),
            description: "Read a file".into(),
            parameters: json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        }];

        let request = build_chat_request(&cfg, "openrouter/auto", &messages, &tools);
        assert!(request.is_ok(), "request should build for valid tool flow");
    }

    #[test]
    fn build_chat_request_accepts_messages_without_tools() {
        let mut cfg = test_config();
        cfg.enable_thinking = Some(false);
        cfg.top_p = None;
        cfg.frequency_penalty = None;

        let messages = vec![ProviderMessage::User {
            content: "hello".into(),
        }];

        let request = build_chat_request(&cfg, "openrouter/auto", &messages, &[]);
        assert!(
            request.is_ok(),
            "request should build without tool metadata"
        );
    }

    #[test]
    fn openrouter_model_mapping_carries_context_length() {
        let info = map_openrouter_model_info(Model {
            id: "openai/gpt-4o-mini".into(),
            name: "GPT-4o mini".into(),
            created: 0.0,
            description: "test".into(),
            context_length: 128000.0,
            architecture: Architecture {
                modality: "text+image->text".into(),
                tokenizer: "cl100k_base".into(),
                instruct_type: None,
            },
            top_provider: TopProvider {
                context_length: Some(128000.0),
                max_completion_tokens: None,
                is_moderated: false,
            },
            pricing: Pricing {
                prompt: "0".into(),
                completion: "0".into(),
                image: None,
                request: None,
                input_cache_read: None,
                input_cache_write: None,
                web_search: None,
                internal_reasoning: None,
            },
            per_request_limits: None,
        });

        assert_eq!(info.context_length, Some(128000));
    }

    #[test]
    fn done_event_emits_usage_before_turn_complete() {
        let (tx, rx) = std::sync::mpsc::channel();

        assert!(!handle_stream_event(
            &tx,
            StreamEvent::Done {
                tool_calls: vec![],
                finish_reason: None,
                usage: Some(ResponseUsage {
                    prompt_tokens: 11,
                    completion_tokens: 7,
                    total_tokens: 18,
                }),
                id: "resp-1".into(),
                model: "openai/gpt-4o-mini".into(),
            }
        ));

        assert_eq!(
            rx.recv().expect("usage event should be emitted"),
            ProviderEvent::Usage(ProviderUsage {
                prompt_tokens: 11,
                completion_tokens: 7,
                total_tokens: 18,
            })
        );
        assert_eq!(
            rx.recv().expect("turn completion should follow usage"),
            ProviderEvent::TurnComplete
        );
    }
}
