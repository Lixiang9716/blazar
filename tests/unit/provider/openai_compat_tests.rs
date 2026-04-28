use super::*;
use crate::agent::tools::ToolSpec;
use async_openai::types::models::Model;
use serde_json::json;

fn test_config() -> OpenAiConfig {
    OpenAiConfig {
        provider_type: Some("openai".to_owned()),
        api_key: "test-key".to_owned(),
        base_url: "http://localhost:1".to_owned(),
        model: "test-model".to_owned(),
        max_tokens: 1024,
        temperature: 0.2,
        top_p: Some(0.8),
        top_k: Some(40.0),
        frequency_penalty: Some(0.1),
        enable_thinking: Some(true),
        thinking_budget: Some(256),
        system_prompt: Some("System prompt".to_owned()),
        system_prompt_file: None,
    }
}

fn test_tools() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "read_file".to_owned(),
        description: "Read a file".to_owned(),
        parameters: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        }),
    }]
}

fn deepseek_config() -> OpenAiConfig {
    OpenAiConfig {
        provider_type: Some("deepseek".to_owned()),
        api_key: "test-key".to_owned(),
        base_url: "https://api.deepseek.com".to_owned(),
        model: "deepseek-v4-pro".to_owned(),
        max_tokens: 1024,
        temperature: 0.2,
        top_p: Some(0.8),
        top_k: Some(40.0),
        frequency_penalty: Some(0.1),
        enable_thinking: Some(true),
        thinking_budget: Some(256),
        system_prompt: Some("System prompt".to_owned()),
        system_prompt_file: None,
    }
}

fn test_provider() -> OpenAiProvider {
    OpenAiProvider::new(test_config()).expect("test provider should initialize")
}

fn sample_messages() -> Vec<ProviderMessage> {
    vec![ProviderMessage::User {
        content: "hello".to_owned(),
    }]
}

#[test]
fn openai_compat_model_mapping_leaves_context_length_unknown() {
    let info = map_openai_model_info(Model {
        id: "gpt-4o-mini".into(),
        object: "model".into(),
        created: 0,
        owned_by: "openai".into(),
    });

    assert_eq!(info.id, "gpt-4o-mini");
    assert_eq!(info.description, "gpt-4o-mini");
    assert_eq!(info.context_length, None);
}

#[test]
fn merge_tool_call_fragment_accumulates_partial_fields() {
    let mut calls = Vec::new();
    let first = DeltaToolCall {
        index: 0,
        id: Some("call-1".to_owned()),
        call_type: Some("function".to_owned()),
        function: Some(DeltaFunction {
            name: Some("read_file".to_owned()),
            arguments: Some("{\"path\":\"a".to_owned()),
        }),
    };
    let second = DeltaToolCall {
        index: 0,
        id: None,
        call_type: None,
        function: Some(DeltaFunction {
            name: None,
            arguments: Some(".txt\"}".to_owned()),
        }),
    };

    merge_tool_call_fragment(&mut calls, &first);
    merge_tool_call_fragment(&mut calls, &second);

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, "call-1");
    assert_eq!(calls[0].function.name, "read_file");
    assert_eq!(calls[0].function.arguments, "{\"path\":\"a.txt\"}");
}

#[test]
fn merge_tool_call_fragment_expands_vector_to_index() {
    let mut calls = Vec::new();
    let fragment = DeltaToolCall {
        index: 2,
        id: Some("call-3".to_owned()),
        call_type: Some("function".to_owned()),
        function: Some(DeltaFunction {
            name: Some("list_dir".to_owned()),
            arguments: Some("{\"path\":\".\"}".to_owned()),
        }),
    };

    merge_tool_call_fragment(&mut calls, &fragment);

    assert_eq!(calls.len(), 3);
    assert_eq!(calls[2].id, "call-3");
    assert_eq!(calls[2].function.name, "list_dir");
}

#[test]
fn collect_tool_call_batch_collects_only_consecutive_tool_calls() {
    let messages = vec![
        ProviderMessage::User {
            content: "hi".to_owned(),
        },
        ProviderMessage::ToolCall {
            id: "call-1".to_owned(),
            name: "read_file".to_owned(),
            arguments: "{}".to_owned(),
        },
        ProviderMessage::ToolCall {
            id: "call-2".to_owned(),
            name: "list_dir".to_owned(),
            arguments: "{}".to_owned(),
        },
        ProviderMessage::Assistant {
            content: "done".to_owned(),
        },
    ];

    let (calls, next) = collect_tool_call_batch(&messages, 1);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].id, "call-1");
    assert_eq!(calls[1].id, "call-2");
    assert_eq!(next, 3);
}

#[test]
fn truncate_provider_messages_limits_recent_context() {
    let mut messages = Vec::new();
    for i in 0..25 {
        messages.push(ProviderMessage::User {
            content: format!("user-{i}"),
        });
        messages.push(ProviderMessage::Assistant {
            content: format!("assistant-{i}"),
        });
        messages.push(ProviderMessage::ToolCall {
            id: format!("call-{i}"),
            name: "read_file".to_owned(),
            arguments: format!("{{\"i\":{i}}}"),
        });
        messages.push(ProviderMessage::ToolResult {
            tool_call_id: format!("call-{i}"),
            output: format!("out-{i}"),
            is_error: false,
        });
    }

    let truncated = truncate_provider_messages(&messages);
    assert!(truncated.len() <= 80);
    assert!(matches!(
        truncated.first(),
        Some(ProviderMessage::User { .. })
    ));
}

#[test]
fn determine_tool_choice_none_when_tools_absent() {
    let messages = vec![ProviderMessage::User {
        content: "hello".to_owned(),
    }];
    assert_eq!(determine_tool_choice(&messages, false), None);
}

#[test]
fn determine_tool_choice_auto_when_no_repeat_success() {
    let messages = vec![
        ProviderMessage::ToolCall {
            id: "c1".to_owned(),
            name: "read_file".to_owned(),
            arguments: "{\"path\":\"a.txt\"}".to_owned(),
        },
        ProviderMessage::ToolResult {
            tool_call_id: "c1".to_owned(),
            output: "content".to_owned(),
            is_error: false,
        },
    ];
    assert_eq!(
        determine_tool_choice(&messages, true),
        Some(ToolChoice::Auto)
    );
}

#[test]
fn determine_tool_choice_none_when_repeat_success_detected() {
    let messages = vec![
        ProviderMessage::ToolCall {
            id: "c1".to_owned(),
            name: "read_file".to_owned(),
            arguments: "{\"path\":\"a.txt\"}".to_owned(),
        },
        ProviderMessage::ToolResult {
            tool_call_id: "c1".to_owned(),
            output: "same-output".to_owned(),
            is_error: false,
        },
        ProviderMessage::ToolCall {
            id: "c2".to_owned(),
            name: "read_file".to_owned(),
            arguments: "{\"path\":\"a.txt\"}".to_owned(),
        },
        ProviderMessage::ToolResult {
            tool_call_id: "c2".to_owned(),
            output: "same-output".to_owned(),
            is_error: false,
        },
    ];
    assert_eq!(
        determine_tool_choice(&messages, true),
        Some(ToolChoice::None)
    );
}

#[test]
fn run_git_command_returns_none_for_invalid_subcommand() {
    let cwd = std::env::current_dir().expect("cwd should resolve");
    assert!(run_git_command(&cwd, &["definitely-not-a-git-command"]).is_none());
}

#[test]
fn render_system_prompt_appends_runtime_context_block() {
    let rendered = render_system_prompt("Base prompt");
    assert!(rendered.starts_with("Base prompt"));
    assert!(rendered.contains("## Runtime Context"));
}

#[test]
fn build_request_serializes_messages_tools_and_tool_choice() {
    let provider = OpenAiProvider::new(test_config()).expect("test provider");
    let messages = vec![
        ProviderMessage::User {
            content: "read file".to_owned(),
        },
        ProviderMessage::Assistant {
            content: "calling tool".to_owned(),
        },
        ProviderMessage::ToolCall {
            id: "call-1".to_owned(),
            name: "read_file".to_owned(),
            arguments: "{\"path\":\"a.txt\"}".to_owned(),
        },
        ProviderMessage::ToolResult {
            tool_call_id: "call-1".to_owned(),
            output: "hello".to_owned(),
            is_error: false,
        },
    ];

    let request = provider.build_request_for_test(&messages, &test_tools());
    let model = request["model"]
        .as_str()
        .expect("request should include model as string");
    let stream = request["stream"]
        .as_bool()
        .expect("request should include stream bool");
    let has_tools = request["tools"].is_array();
    let choice = request["tool_choice"]
        .as_str()
        .expect("request should include tool_choice");
    let msg_count = request["messages"]
        .as_array()
        .expect("messages should be array")
        .len();

    assert_eq!(model, "test-model");
    assert!(stream);
    assert!(has_tools);
    assert_eq!(choice, "auto");
    assert!(msg_count >= 3);
}

#[test]
fn build_request_uses_deepseek_thinking_fields_without_tools() {
    let provider = OpenAiProvider::new(deepseek_config()).expect("test provider");
    let request = provider.build_request_for_test(
        &[ProviderMessage::User {
            content: "hello".to_owned(),
        }],
        &[],
    );

    assert_eq!(request["model"], "deepseek-v4-pro");
    assert_eq!(request["thinking"]["type"], "enabled");
    assert_eq!(request["reasoning_effort"], "high");
    assert!(request.get("enable_thinking").is_none());
    assert!(request.get("thinking_budget").is_none());
}

#[test]
fn build_request_disables_deepseek_thinking_when_tools_are_available() {
    let provider = OpenAiProvider::new(deepseek_config()).expect("test provider");
    let request = provider.build_request_for_test(
        &[ProviderMessage::User {
            content: "inspect Cargo.toml".to_owned(),
        }],
        &test_tools(),
    );

    assert_eq!(request["thinking"]["type"], "disabled");
    assert!(request.get("reasoning_effort").is_none());
    assert!(request.get("enable_thinking").is_none());
    assert!(request.get("thinking_budget").is_none());
}

#[test]
fn build_request_sets_stream_include_usage() {
    let provider = test_provider();
    let req = provider.build_request_for_test(&sample_messages(), &[]);

    assert_eq!(req["stream_options"]["include_usage"], json!(true));
}

#[test]
fn stream_chunk_deserializes_content_without_id_field() {
    let chunk = serde_json::from_str::<StreamChunk>(
        r#"{
            "choices": [
                {
                    "index": 0,
                    "delta": {
                        "content": "hello"
                    },
                    "finish_reason": null
                }
            ]
        }"#,
    )
    .expect("stream chunk without id should still deserialize");

    assert_eq!(chunk.choices.len(), 1);
    assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("hello"));
}

#[test]
fn usage_chunk_emits_provider_usage_event() {
    let chunk: StreamChunk = serde_json::from_value(json!({
        "choices": [],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 20,
            "total_tokens": 120
        }
    }))
    .expect("chunk should parse");

    let (tx, rx) = std::sync::mpsc::channel();
    let mut tool_calls = Vec::new();
    let mut event_count = 0;

    assert!(emit_chunk_events(
        &tx,
        &chunk,
        &mut tool_calls,
        &mut event_count
    ));
    assert_eq!(
        rx.recv().expect("usage event should be emitted"),
        ProviderEvent::Usage(crate::provider::ProviderUsage {
            prompt_tokens: 100,
            completion_tokens: 20,
            total_tokens: 120,
        })
    );
}
