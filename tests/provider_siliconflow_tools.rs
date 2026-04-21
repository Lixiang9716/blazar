#[path = "common/http_test_server.rs"]
mod http_test_server;

use blazar::agent::tools::ToolSpec;
use blazar::provider::siliconflow::{
    DeltaFunction, DeltaToolCall, FunctionCall, SiliconFlowConfig, SiliconFlowProvider, ToolCall,
    ToolChoice, merge_tool_call_fragment,
};
use blazar::provider::{LlmProvider, ProviderMessage};
use http_test_server::{http_response, spawn_one_shot_server};
use serde_json::json;
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn build_request_groups_tool_only_multi_call_turns() {
    let provider = SiliconFlowProvider::new(SiliconFlowConfig {
        api_key: "test".into(),
        base_url: "https://example.com/v1".into(),
        model: "Qwen/Qwen3-8B".into(),
        max_tokens: 256,
        temperature: 0.0,
        top_p: None,
        top_k: None,
        frequency_penalty: None,
        enable_thinking: None,
        thinking_budget: None,
        system_prompt: Some("system".into()),
        system_prompt_file: None,
    });

    let request = provider.build_request_for_test(
        &[
            ProviderMessage::User {
                content: "show Cargo.toml".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-1".into(),
                name: "read_file".into(),
                arguments: "{\"path\":\"Cargo.toml\"}".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-2".into(),
                name: "read_file".into(),
                arguments: "{\"path\":\"src/main.rs\"}".into(),
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-1".into(),
                output: "cargo contents".into(),
                is_error: false,
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-2".into(),
                output: "main contents".into(),
                is_error: false,
            },
        ],
        &[ToolSpec {
            name: "read_file".into(),
            description: "Read a file".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        }],
    );

    assert_eq!(request.messages.len(), 5);
    assert_eq!(
        request.tools.as_ref().unwrap()[0].function.name,
        "read_file"
    );
    assert_eq!(
        request.messages[2].role,
        blazar::provider::siliconflow::Role::Assistant
    );
    assert!(request.messages[2].content.is_none());
    assert_eq!(request.messages[2].tool_calls.as_ref().unwrap().len(), 2);
    assert_eq!(request.messages[3].tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(request.messages[4].tool_call_id.as_deref(), Some("call-2"));
}

#[test]
fn build_request_preserves_assistant_text_on_tool_turn() {
    let provider = SiliconFlowProvider::new(SiliconFlowConfig {
        api_key: "test".into(),
        base_url: "https://example.com/v1".into(),
        model: "Qwen/Qwen3-8B".into(),
        max_tokens: 256,
        temperature: 0.0,
        top_p: None,
        top_k: None,
        frequency_penalty: None,
        enable_thinking: None,
        thinking_budget: None,
        system_prompt: Some("system".into()),
        system_prompt_file: None,
    });

    let request = provider.build_request_for_test(
        &[
            ProviderMessage::User {
                content: "compare two files".into(),
            },
            ProviderMessage::Assistant {
                content: "I'll inspect both files.".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-1".into(),
                name: "read_file".into(),
                arguments: "{\"path\":\"a.txt\"}".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-2".into(),
                name: "read_file".into(),
                arguments: "{\"path\":\"b.txt\"}".into(),
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-1".into(),
                output: "alpha".into(),
                is_error: false,
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-2".into(),
                output: "beta".into(),
                is_error: false,
            },
        ],
        &[],
    );

    assert_eq!(request.messages.len(), 5);
    assert_eq!(
        request.messages[2].content.as_deref(),
        Some("I'll inspect both files.")
    );
    assert_eq!(request.messages[2].tool_calls.as_ref().unwrap().len(), 2);
}

#[test]
fn build_request_injects_runtime_context_into_system_prompt() {
    let provider = SiliconFlowProvider::new(SiliconFlowConfig {
        api_key: "test".into(),
        base_url: "https://example.com/v1".into(),
        model: "Qwen/Qwen3-8B".into(),
        max_tokens: 256,
        temperature: 0.0,
        top_p: None,
        top_k: None,
        frequency_penalty: None,
        enable_thinking: None,
        thinking_budget: None,
        system_prompt: Some("base system".into()),
        system_prompt_file: None,
    });

    let request = provider.build_request_for_test(
        &[ProviderMessage::User {
            content: "hello".into(),
        }],
        &[],
    );

    let system_content = request.messages[0]
        .content
        .as_deref()
        .expect("system message should have content");
    assert!(system_content.contains("base system"));
    assert!(system_content.contains("## Runtime Context"));
    assert!(system_content.contains("Working directory:"));
}

#[test]
fn merge_tool_call_fragment_concatenates_streamed_arguments() {
    let mut tool_calls = vec![ToolCall {
        id: String::new(),
        call_type: "function".into(),
        function: FunctionCall {
            name: String::new(),
            arguments: String::new(),
        },
    }];

    merge_tool_call_fragment(
        &mut tool_calls,
        &DeltaToolCall {
            index: 0,
            id: Some("call-1".into()),
            call_type: Some("function".into()),
            function: Some(DeltaFunction {
                name: Some("read_file".into()),
                arguments: Some("{\"path\":\"Ca".into()),
            }),
        },
    );
    merge_tool_call_fragment(
        &mut tool_calls,
        &DeltaToolCall {
            index: 0,
            id: None,
            call_type: None,
            function: Some(DeltaFunction {
                name: None,
                arguments: Some("rgo.toml\"}".into()),
            }),
        },
    );

    assert_eq!(tool_calls[0].id, "call-1");
    assert_eq!(tool_calls[0].function.name, "read_file");
    assert_eq!(
        tool_calls[0].function.arguments,
        "{\"path\":\"Cargo.toml\"}"
    );
}

#[test]
fn build_request_uses_auto_tool_choice_when_tools_available() {
    let provider = SiliconFlowProvider::new(SiliconFlowConfig {
        api_key: "test".into(),
        base_url: "https://example.com/v1".into(),
        model: "Qwen/Qwen3-8B".into(),
        max_tokens: 256,
        temperature: 0.0,
        top_p: None,
        top_k: None,
        frequency_penalty: None,
        enable_thinking: None,
        thinking_budget: None,
        system_prompt: Some("system".into()),
        system_prompt_file: None,
    });

    let request = provider.build_request_for_test(
        &[ProviderMessage::User {
            content: "read file".into(),
        }],
        &[ToolSpec {
            name: "read_file".into(),
            description: "Read a file".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        }],
    );

    assert_eq!(request.tool_choice, Some(ToolChoice::Auto));
}

#[test]
fn build_request_switches_tool_choice_to_none_after_repeated_success() {
    let provider = SiliconFlowProvider::new(SiliconFlowConfig {
        api_key: "test".into(),
        base_url: "https://example.com/v1".into(),
        model: "Qwen/Qwen3-8B".into(),
        max_tokens: 256,
        temperature: 0.0,
        top_p: None,
        top_k: None,
        frequency_penalty: None,
        enable_thinking: None,
        thinking_budget: None,
        system_prompt: Some("system".into()),
        system_prompt_file: None,
    });

    let request = provider.build_request_for_test(
        &[
            ProviderMessage::User {
                content: "write hello".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-1".into(),
                name: "write_file".into(),
                arguments: "{\"path\":\"hello.py\",\"content\":\"print('hello')\"}".into(),
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-1".into(),
                output: "Wrote 26 bytes to hello.py".into(),
                is_error: false,
            },
            ProviderMessage::ToolCall {
                id: "call-2".into(),
                name: "write_file".into(),
                arguments: "{\"path\":\"hello.py\",\"content\":\"print('hello')\"}".into(),
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-2".into(),
                output: "Wrote 26 bytes to hello.py".into(),
                is_error: false,
            },
        ],
        &[ToolSpec {
            name: "write_file".into(),
            description: "Write file".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        }],
    );

    assert_eq!(request.tool_choice, Some(ToolChoice::None));
}

#[test]
fn build_request_truncates_old_context_to_recent_user_turns() {
    let provider = SiliconFlowProvider::new(SiliconFlowConfig {
        api_key: "test".into(),
        base_url: "https://example.com/v1".into(),
        model: "Qwen/Qwen3-8B".into(),
        max_tokens: 256,
        temperature: 0.0,
        top_p: None,
        top_k: None,
        frequency_penalty: None,
        enable_thinking: None,
        thinking_budget: None,
        system_prompt: Some("system".into()),
        system_prompt_file: None,
    });

    let mut messages = Vec::new();
    for turn in 0..15 {
        messages.push(ProviderMessage::User {
            content: format!("user-{turn}"),
        });
        messages.push(ProviderMessage::Assistant {
            content: format!("assistant-{turn}"),
        });
    }

    let request = provider.build_request_for_test(&messages, &[]);

    // MAX_CONTEXT_USER_TURNS=10: keeps turns 5-14 (10 user + 10 assistant + 1 system = 21).
    assert_eq!(request.messages.len(), 21);
    assert_eq!(request.messages[1].content.as_deref(), Some("user-5"));
    assert_eq!(
        request.messages[20].content.as_deref(),
        Some("assistant-14")
    );
}

#[test]
fn build_request_switches_tool_choice_to_none_for_batched_repeated_success() {
    let provider = SiliconFlowProvider::new(SiliconFlowConfig {
        api_key: "test".into(),
        base_url: "https://example.com/v1".into(),
        model: "Qwen/Qwen3-8B".into(),
        max_tokens: 256,
        temperature: 0.0,
        top_p: None,
        top_k: None,
        frequency_penalty: None,
        enable_thinking: None,
        thinking_budget: None,
        system_prompt: Some("system".into()),
        system_prompt_file: None,
    });

    let request = provider.build_request_for_test(
        &[
            ProviderMessage::User {
                content: "write and run".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-1".into(),
                name: "write_file".into(),
                arguments: "{\"path\":\"hello.py\",\"content\":\"print('hello')\"}".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-2".into(),
                name: "bash".into(),
                arguments: "{\"command\":\"python hello.py\"}".into(),
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-1".into(),
                output: "Wrote 26 bytes to hello.py".into(),
                is_error: false,
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-2".into(),
                output: "hello".into(),
                is_error: false,
            },
            ProviderMessage::ToolCall {
                id: "call-3".into(),
                name: "write_file".into(),
                arguments: "{\"path\":\"hello.py\",\"content\":\"print('hello')\"}".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-4".into(),
                name: "bash".into(),
                arguments: "{\"command\":\"python hello.py\"}".into(),
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-3".into(),
                output: "Wrote 26 bytes to hello.py".into(),
                is_error: false,
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-4".into(),
                output: "hello".into(),
                is_error: false,
            },
        ],
        &[ToolSpec {
            name: "write_file".into(),
            description: "Write file".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        }],
    );

    assert_eq!(request.tool_choice, Some(ToolChoice::None));
}

#[test]
fn build_request_does_not_false_positive_tool_choice_none_on_mismatched_batches() {
    let provider = SiliconFlowProvider::new(SiliconFlowConfig {
        api_key: "test".into(),
        base_url: "https://example.com/v1".into(),
        model: "Qwen/Qwen3-8B".into(),
        max_tokens: 256,
        temperature: 0.0,
        top_p: None,
        top_k: None,
        frequency_penalty: None,
        enable_thinking: None,
        thinking_budget: None,
        system_prompt: Some("system".into()),
        system_prompt_file: None,
    });

    let request = provider.build_request_for_test(
        &[
            ProviderMessage::User {
                content: "batch run".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-1".into(),
                name: "write_file".into(),
                arguments: "{\"path\":\"hello.py\",\"content\":\"print('a')\"}".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-2".into(),
                name: "bash".into(),
                arguments: "{\"command\":\"python hello.py\"}".into(),
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-1".into(),
                output: "OK".into(),
                is_error: false,
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-2".into(),
                output: "run-a".into(),
                is_error: false,
            },
            ProviderMessage::ToolCall {
                id: "call-3".into(),
                name: "write_file".into(),
                arguments: "{\"path\":\"hello.py\",\"content\":\"print('b')\"}".into(),
            },
            ProviderMessage::ToolCall {
                id: "call-4".into(),
                name: "bash".into(),
                arguments: "{\"command\":\"python hello.py\"}".into(),
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-3".into(),
                output: "OK".into(),
                is_error: false,
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call-4".into(),
                output: "run-b".into(),
                is_error: false,
            },
        ],
        &[ToolSpec {
            name: "write_file".into(),
            description: "Write file".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        }],
    );

    assert_eq!(request.tool_choice, Some(ToolChoice::Auto));
}

#[test]
fn load_config_defaults_enable_thinking_to_false() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let repo_root = std::env::temp_dir().join(format!("blazar-provider-config-{nonce}"));
    std::fs::create_dir_all(repo_root.join("config")).expect("create config dir");
    std::fs::write(
        repo_root.join("config/provider.json"),
        r#"{
  "api_key": "test-key",
  "base_url": "https://example.com/v1",
  "model": "Qwen/Qwen3-8B"
}"#,
    )
    .expect("write provider config");

    let config = SiliconFlowConfig::load(repo_root.to_str().expect("utf-8 temp path"))
        .expect("load provider config");
    assert_eq!(config.enable_thinking, Some(false));

    std::fs::remove_dir_all(&repo_root).expect("cleanup temp repo");
}

#[test]
fn load_config_reads_system_prompt_file_and_overrides_inline_prompt() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let repo_root = std::env::temp_dir().join(format!("blazar-provider-config-file-{nonce}"));
    std::fs::create_dir_all(repo_root.join("config")).expect("create config dir");
    std::fs::create_dir_all(repo_root.join("prompts")).expect("create prompts dir");
    std::fs::write(
        repo_root.join("prompts/system.md"),
        "You are a careful coding assistant.",
    )
    .expect("write prompt file");
    std::fs::write(
        repo_root.join("config/provider.json"),
        r#"{
  "api_key": "test-key",
  "base_url": "https://example.com/v1",
  "model": "Qwen/Qwen3-8B",
  "system_prompt": "inline prompt",
  "system_prompt_file": "prompts/system.md"
}"#,
    )
    .expect("write provider config");

    let config = SiliconFlowConfig::load(repo_root.to_str().expect("utf-8 temp path"))
        .expect("load provider config");
    assert_eq!(
        config.system_prompt.as_deref(),
        Some("You are a careful coding assistant.")
    );
    assert_eq!(config.enable_thinking, Some(false));

    std::fs::remove_dir_all(&repo_root).expect("cleanup temp repo");
}

#[test]
fn load_config_returns_descriptive_errors_for_missing_or_invalid_input() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let missing_root = std::env::temp_dir().join(format!("blazar-provider-missing-{nonce}"));
    std::fs::create_dir_all(&missing_root).expect("create root");
    let missing_err = SiliconFlowConfig::load(missing_root.to_str().expect("utf-8 temp path"))
        .expect_err("missing config should fail");
    assert!(missing_err.contains("cannot read"));

    let invalid_root = std::env::temp_dir().join(format!("blazar-provider-invalid-{nonce}"));
    std::fs::create_dir_all(invalid_root.join("config")).expect("create config dir");
    std::fs::write(invalid_root.join("config/provider.json"), "{not-json")
        .expect("write invalid json");
    let invalid_err = SiliconFlowConfig::load(invalid_root.to_str().expect("utf-8 temp path"))
        .expect_err("invalid json should fail");
    assert!(invalid_err.contains("invalid provider config"));

    std::fs::write(
        invalid_root.join("config/provider.json"),
        r#"{
  "api_key": "test-key",
  "base_url": "https://example.com/v1",
  "model": "Qwen/Qwen3-8B",
  "system_prompt_file": "missing/prompt.md"
}"#,
    )
    .expect("write config with missing prompt file");
    let prompt_err = SiliconFlowConfig::load(invalid_root.to_str().expect("utf-8 temp path"))
        .expect_err("missing prompt file should fail");
    assert!(prompt_err.contains("cannot read system_prompt_file"));

    std::fs::remove_dir_all(&missing_root).expect("cleanup missing root");
    std::fs::remove_dir_all(&invalid_root).expect("cleanup invalid root");
}

#[test]
fn merge_tool_call_fragment_expands_vector_for_sparse_indexes() {
    let mut tool_calls = Vec::new();
    merge_tool_call_fragment(
        &mut tool_calls,
        &DeltaToolCall {
            index: 2,
            id: Some("call-3".into()),
            call_type: Some("function".into()),
            function: Some(DeltaFunction {
                name: Some("bash".into()),
                arguments: Some("{\"command\":\"echo hi\"}".into()),
            }),
        },
    );

    assert_eq!(tool_calls.len(), 3);
    assert_eq!(tool_calls[2].id, "call-3");
    assert_eq!(tool_calls[2].function.name, "bash");
    assert_eq!(
        tool_calls[2].function.arguments,
        "{\"command\":\"echo hi\"}"
    );
    assert!(tool_calls[0].id.is_empty());
}

#[test]
fn stream_turn_surfaces_provider_errors_and_success_events() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    drop(listener);

    let error_provider = SiliconFlowProvider::new(SiliconFlowConfig {
        api_key: "test".into(),
        base_url: format!("http://{addr}"),
        model: "Qwen/Qwen3-8B".into(),
        max_tokens: 64,
        temperature: 0.0,
        top_p: None,
        top_k: None,
        frequency_penalty: None,
        enable_thinking: None,
        thinking_budget: None,
        system_prompt: None,
        system_prompt_file: None,
    });

    let (tx, rx) = mpsc::channel();
    error_provider.stream_turn(
        &[ProviderMessage::User {
            content: "hello".into(),
        }],
        &[],
        tx,
    );
    let events: Vec<_> = rx.try_iter().collect();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, blazar::provider::ProviderEvent::Error(_)))
    );

    let (base_url, handle) = spawn_one_shot_server(|request| {
        assert!(request.starts_with("POST /chat/completions HTTP/1.1"));
        http_response(200, "OK", "text/event-stream", "data: [DONE]\n")
    });
    let success_provider = SiliconFlowProvider::new(SiliconFlowConfig {
        api_key: "test".into(),
        base_url,
        model: "Qwen/Qwen3-8B".into(),
        max_tokens: 64,
        temperature: 0.0,
        top_p: None,
        top_k: None,
        frequency_penalty: None,
        enable_thinking: None,
        thinking_budget: None,
        system_prompt: None,
        system_prompt_file: None,
    });

    let (tx, rx) = mpsc::channel();
    success_provider.stream_turn(
        &[ProviderMessage::User {
            content: "hello".into(),
        }],
        &[],
        tx,
    );
    handle.join().expect("server joined");
    let events: Vec<_> = rx.try_iter().collect();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, blazar::provider::ProviderEvent::TurnComplete))
    );
}
