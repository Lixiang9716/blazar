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
use super::{LlmProvider, ModelInfo, ProviderEvent, ProviderMessage};

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

        let truncated = truncate_provider_messages(messages);

        // Build openrouter-rs messages
        let mut or_messages: Vec<OrMessage> = Vec::new();

        // Inject system prompt
        if let Some(ref sys) = self.config.system_prompt {
            or_messages.push(OrMessage::new(OrRole::System, render_system_prompt(sys)));
        }

        // Convert ProviderMessage → OrMessage
        let mut idx = 0usize;
        while idx < truncated.len() {
            match &truncated[idx] {
                ProviderMessage::User { content } => {
                    or_messages.push(OrMessage::new(OrRole::User, content.as_str()));
                    idx += 1;
                }
                ProviderMessage::Assistant { content } => {
                    // Collect any immediately following ToolCall messages
                    let (tool_calls, next_idx) = collect_tool_calls(&truncated, idx + 1);
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
                    // Orphan ToolCall (no preceding Assistant) — wrap it
                    let (tool_calls, next_idx) = collect_tool_calls(&truncated, idx);
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

        // Convert ToolSpec → OrTool
        let or_tools: Vec<OrTool> = tools
            .iter()
            .map(|t| OrTool::new(&t.name, &t.description, t.parameters.clone()))
            .collect();

        // Build request
        let mut builder = ChatCompletionRequest::builder();
        builder
            .model(model)
            .messages(or_messages)
            .max_tokens(self.config.max_tokens)
            .temperature(f64::from(self.config.temperature));

        if let Some(top_p) = self.config.top_p {
            builder.top_p(f64::from(top_p));
        }
        if let Some(fp) = self.config.frequency_penalty {
            builder.frequency_penalty(f64::from(fp));
        }

        // Tools & tool choice
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

        // Reasoning / thinking mode
        if self.config.enable_thinking == Some(true) {
            builder.enable_reasoning();
        }

        let request = match builder.build() {
            Ok(req) => req,
            Err(e) => {
                let _ = tx.send(ProviderEvent::Error(format!("request build error: {e}")));
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
                match event {
                    StreamEvent::ContentDelta(text) => {
                        if tx.send(ProviderEvent::TextDelta(text)).is_err() {
                            return Ok(());
                        }
                    }
                    StreamEvent::ReasoningDelta(text) => {
                        if tx.send(ProviderEvent::ThinkingDelta(text)).is_err() {
                            return Ok(());
                        }
                    }
                    StreamEvent::ReasoningDetailsDelta(_) => {
                        // Structured reasoning details — skip for now
                    }
                    StreamEvent::Done { tool_calls, .. } => {
                        for tc in tool_calls {
                            let _ = tx.send(ProviderEvent::ToolCall {
                                call_id: tc.id().to_owned(),
                                name: tc.name().to_owned(),
                                arguments: tc.arguments_json().to_owned(),
                            });
                        }
                        let _ = tx.send(ProviderEvent::TurnComplete);
                        return Ok(());
                    }
                    StreamEvent::Error(e) => {
                        let _ = tx.send(ProviderEvent::Error(format!("{e}")));
                        return Ok(());
                    }
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

        let mut result: Vec<ModelInfo> = models
            .into_iter()
            .map(|m| ModelInfo {
                description: m.name.clone(),
                id: m.id,
            })
            .collect();
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
