use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use blazar::agent::tools::{
    BuiltinToolDescriptor, BuiltinToolProfiles, Tool, ToolBuildContext, ToolBuildProfile,
    ToolResult, ToolSpec, collect_and_build_builtins, register_builtin_tools,
};
use blazar::provider::{LlmProvider, ProviderEvent, ProviderMessage};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Minimal stub provider — only needed to construct ToolBuildContext
// ---------------------------------------------------------------------------

struct StubProvider;

impl LlmProvider for StubProvider {
    fn stream_turn(
        &self,
        _model: &str,
        _messages: &[ProviderMessage],
        _tools: &[ToolSpec],
        _tx: Sender<ProviderEvent>,
    ) {
    }
}

fn test_ctx() -> ToolBuildContext {
    ToolBuildContext {
        workspace_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        provider: Arc::new(StubProvider),
        model: "test-model".into(),
    }
}

// ---------------------------------------------------------------------------
// Positive tests — exercise inventory-backed registration
// ---------------------------------------------------------------------------

#[test]
fn main_runtime_provides_expected_tools() {
    let ctx = test_ctx();
    let registry = register_builtin_tools(&ctx, ToolBuildProfile::MainRuntime)
        .expect("main runtime assembly should succeed");

    let mut names: Vec<String> = registry.specs().iter().map(|s| s.name.clone()).collect();
    names.sort();

    assert_eq!(
        names,
        vec![
            "bash",
            "list_dir",
            "read_file",
            "sub_agent",
            "vet",
            "write_file"
        ],
    );
}

#[test]
fn sub_agent_provides_expected_tools() {
    let ctx = test_ctx();
    let registry = register_builtin_tools(&ctx, ToolBuildProfile::SubAgent)
        .expect("sub-agent assembly should succeed");

    let mut names: Vec<String> = registry.specs().iter().map(|s| s.name.clone()).collect();
    names.sort();

    assert_eq!(names, vec!["bash", "list_dir", "read_file", "write_file"],);
}

#[test]
fn tools_are_sorted_by_name() {
    let ctx = test_ctx();
    let registry = register_builtin_tools(&ctx, ToolBuildProfile::MainRuntime)
        .expect("assembly should succeed");

    let names: Vec<String> = registry.specs().iter().map(|s| s.name.clone()).collect();
    let mut sorted = names.clone();
    sorted.sort();

    assert_eq!(
        names, sorted,
        "tools should be registered in alphabetical order"
    );
}

// ---------------------------------------------------------------------------
// Negative tests — use collect_and_build_builtins with synthetic descriptors
// ---------------------------------------------------------------------------

fn stub_build(_ctx: &ToolBuildContext) -> Box<dyn Tool> {
    struct InlineTool(&'static str);
    impl Tool for InlineTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: self.0.into(),
                description: "stub".into(),
                parameters: json!({"type": "object", "properties": {}}),
            }
        }
        fn execute(&self, _args: Value) -> ToolResult {
            ToolResult::success("ok")
        }
    }
    Box::new(InlineTool("alpha"))
}

#[test]
fn duplicate_builtin_name_is_rejected() {
    let d1 = BuiltinToolDescriptor {
        name: "alpha",
        profiles: BuiltinToolProfiles::Both,
        build: stub_build,
    };
    let d2 = BuiltinToolDescriptor {
        name: "alpha",
        profiles: BuiltinToolProfiles::Both,
        build: stub_build,
    };

    let ctx = test_ctx();
    let result = collect_and_build_builtins(&[&d1, &d2], &ctx);

    match result {
        Err(e) => assert!(
            e.contains("duplicate"),
            "error should mention duplicate: {e}"
        ),
        Ok(_) => panic!("expected duplicate name error"),
    }
}

#[test]
fn name_mismatch_is_rejected() {
    fn mismatched_build(_ctx: &ToolBuildContext) -> Box<dyn Tool> {
        struct MismatchTool;
        impl Tool for MismatchTool {
            fn spec(&self) -> ToolSpec {
                ToolSpec {
                    name: "actual_name".into(),
                    description: "stub".into(),
                    parameters: json!({"type": "object", "properties": {}}),
                }
            }
            fn execute(&self, _args: Value) -> ToolResult {
                ToolResult::success("ok")
            }
        }
        Box::new(MismatchTool)
    }

    let descriptor = BuiltinToolDescriptor {
        name: "declared_name",
        profiles: BuiltinToolProfiles::Both,
        build: mismatched_build,
    };

    let ctx = test_ctx();
    let result = collect_and_build_builtins(&[&descriptor], &ctx);

    match result {
        Err(e) => assert!(e.contains("mismatch"), "error should mention mismatch: {e}"),
        Ok(_) => panic!("expected name mismatch error"),
    }
}
