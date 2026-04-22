use blazar::agent::tools::{Tool, ToolRegistry, ToolResult, ToolSpec};
use serde_json::{Value, json};
use std::path::PathBuf;

struct StubTool;

impl Tool for StubTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "stub".into(),
            description: "A stub tool".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "value": { "type": "string" }
                },
                "required": ["value"],
                "additionalProperties": false
            }),
        }
    }

    fn execute(&self, args: Value) -> ToolResult {
        ToolResult::success(format!("stub:{}", args["value"].as_str().unwrap()))
    }
}

#[test]
fn registry_lists_registered_specs() {
    let mut registry = ToolRegistry::new(PathBuf::from("/tmp/blazar-registry"));
    registry.register(Box::new(StubTool));

    let specs = registry.specs();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].name, "stub");
}

#[test]
fn registry_executes_registered_tool() {
    let mut registry = ToolRegistry::new(PathBuf::from("/tmp/blazar-registry"));
    registry.register(Box::new(StubTool));

    let result = registry
        .execute("stub", json!({ "value": "ok" }))
        .expect("registered tool should execute");

    assert_eq!(result.text_output(), "stub:ok");
    assert!(!result.is_error);
    assert_eq!(result.exit_code, None);
    assert!(!result.output_truncated);
}

#[test]
fn registry_rejects_unknown_tool() {
    let registry = ToolRegistry::new(PathBuf::from("/tmp/blazar-registry"));

    let error = registry
        .execute("missing", json!({}))
        .expect_err("missing tool should fail");

    assert_eq!(error, "unknown tool: missing");
}
