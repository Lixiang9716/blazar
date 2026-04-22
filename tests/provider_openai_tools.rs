use blazar::agent::tools::ToolSpec;
use blazar::provider::ProviderMessage;
use blazar::provider::openai_compat::{
    DeltaFunction, DeltaToolCall, FunctionCall, OpenAiConfig, OpenAiProvider, ToolCall,
    merge_tool_call_fragment,
};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

fn make_provider() -> OpenAiProvider {
    OpenAiProvider::new(OpenAiConfig {
        provider_type: None,
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
    })
}

#[test]
fn build_request_groups_tool_only_multi_call_turns() {
    let provider = make_provider();

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

    let messages = request["messages"].as_array().unwrap();
    // system + user + assistant(tool_calls) + tool_result + tool_result = 5
    assert_eq!(messages.len(), 5);
    assert_eq!(
        request["tools"][0]["function"]["name"].as_str().unwrap(),
        "read_file"
    );
    assert_eq!(messages[2]["role"].as_str().unwrap(), "assistant");
    assert!(messages[2].get("content").is_none() || messages[2]["content"].is_null());
    assert_eq!(messages[2]["tool_calls"].as_array().unwrap().len(), 2);
    assert_eq!(messages[3]["tool_call_id"].as_str().unwrap(), "call-1");
    assert_eq!(messages[4]["tool_call_id"].as_str().unwrap(), "call-2");
}

#[test]
fn build_request_preserves_assistant_text_on_tool_turn() {
    let provider = make_provider();

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

    let messages = request["messages"].as_array().unwrap();
    // system + user + assistant(tool_calls+content) + tool_result + tool_result = 5
    assert_eq!(messages.len(), 5);
    assert_eq!(
        messages[2]["content"].as_str().unwrap(),
        "I'll inspect both files."
    );
    assert_eq!(messages[2]["tool_calls"].as_array().unwrap().len(), 2);
}

#[test]
fn build_request_injects_runtime_context_into_system_prompt() {
    let provider = make_provider();

    let request = provider.build_request_for_test(
        &[ProviderMessage::User {
            content: "hello".into(),
        }],
        &[],
    );

    let messages = request["messages"].as_array().unwrap();
    let system_content = messages[0]["content"]
        .as_str()
        .expect("system message should have content");
    assert!(system_content.contains("system"));
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
    let provider = make_provider();

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

    assert_eq!(request["tool_choice"].as_str().unwrap(), "auto");
}

#[test]
fn build_request_switches_tool_choice_to_none_after_repeated_success() {
    let provider = make_provider();

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

    assert_eq!(request["tool_choice"].as_str().unwrap(), "none");
}

#[test]
fn build_request_truncates_old_context_to_recent_user_turns() {
    let provider = make_provider();

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
    let req_messages = request["messages"].as_array().unwrap();

    // MAX_CONTEXT_USER_TURNS=10: keeps turns 5-14 (10 user + 10 assistant + 1 system = 21).
    assert_eq!(req_messages.len(), 21);
    assert_eq!(req_messages[1]["content"].as_str().unwrap(), "user-5");
    assert_eq!(
        req_messages[20]["content"].as_str().unwrap(),
        "assistant-14"
    );
}

#[test]
fn build_request_switches_tool_choice_to_none_for_batched_repeated_success() {
    let provider = make_provider();

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

    assert_eq!(request["tool_choice"].as_str().unwrap(), "none");
}

#[test]
fn build_request_does_not_false_positive_tool_choice_none_on_mismatched_batches() {
    let provider = make_provider();

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

    assert_eq!(request["tool_choice"].as_str().unwrap(), "auto");
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

    let config = OpenAiConfig::load(repo_root.to_str().expect("utf-8 temp path"))
        .expect("load provider config");
    assert_eq!(config.enable_thinking, Some(false));

    std::fs::remove_dir_all(&repo_root).expect("cleanup temp repo");
}
