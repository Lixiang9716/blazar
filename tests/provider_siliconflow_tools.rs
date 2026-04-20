use blazar::agent::tools::ToolSpec;
use blazar::provider::ProviderMessage;
use blazar::provider::siliconflow::{
    DeltaFunction, DeltaToolCall, FunctionCall, SiliconFlowConfig, SiliconFlowProvider, ToolCall,
    merge_tool_call_fragment,
};
use serde_json::json;

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
