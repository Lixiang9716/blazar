use std::path::PathBuf;

use serde_json::{Value, json};

use super::bash::BashTool;
use super::{
    BuiltinToolDescriptor, BuiltinToolProfiles, ResourceAccess, ResourceClaim, Tool,
    ToolBuildContext, ToolResult, ToolSpec,
};

inventory::submit! {
    BuiltinToolDescriptor {
        name: "vet",
        profiles: BuiltinToolProfiles::MainOnly,
        build: |ctx: &ToolBuildContext| Box::new(VetTool::new(ctx.workspace_root.clone())),
    }
}

/// Abstraction over shell execution so VetTool can be tested without
/// spawning real processes.
pub(crate) trait ShellExecutor: Send + Sync {
    fn execute_shell(&self, args: Value) -> ToolResult;
}

struct BashShellExecutor {
    workspace_root: PathBuf,
}

impl ShellExecutor for BashShellExecutor {
    fn execute_shell(&self, args: Value) -> ToolResult {
        BashTool::new(self.workspace_root.clone()).execute(args)
    }
}

pub struct VetTool {
    executor: Box<dyn ShellExecutor>,
}

impl VetTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            executor: Box::new(BashShellExecutor { workspace_root }),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_executor(executor: Box<dyn ShellExecutor>) -> Self {
        Self { executor }
    }
}

impl Tool for VetTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "vet".into(),
            description: "Run repository vet checks. Modes: quick (fmt+lint), full (fmt+lint+test), or cargo-vet.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "description": "Vet mode to run.",
                        "enum": ["quick", "full", "cargo-vet"]
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Optional timeout passed to shell execution."
                    }
                },
                "additionalProperties": false
            }),
        }
    }

    fn resource_claims(&self, _args: &Value) -> Vec<ResourceClaim> {
        vec![ResourceClaim {
            resource: "process:bash".into(),
            access: ResourceAccess::Exclusive,
        }]
    }

    fn execute(&self, args: Value) -> ToolResult {
        let mode = args.get("mode").and_then(Value::as_str).unwrap_or("quick");
        let command = match command_for_mode(mode) {
            Ok(command) => command,
            Err(error) => return ToolResult::failure(error),
        };

        let mut shell_args = json!({ "command": command });
        if let Some(timeout_secs) = args.get("timeout_secs").and_then(Value::as_u64) {
            shell_args["timeout_secs"] = Value::from(timeout_secs);
        }

        self.executor.execute_shell(shell_args)
    }
}

fn command_for_mode(mode: &str) -> Result<&'static str, String> {
    match mode {
        "quick" => Ok("just fmt-check && just lint"),
        "full" => Ok("just fmt-check && just lint && just test"),
        "cargo-vet" => Ok("cargo vet"),
        other => Err(format!(
            "vet mode must be one of: quick, full, cargo-vet (got: {other})"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A simple shell executor that captures the args and returns a canned result.
    struct StubShellExecutor {
        captured: Mutex<Vec<Value>>,
        result: ToolResult,
    }

    impl StubShellExecutor {
        fn new(result: ToolResult) -> Self {
            Self {
                captured: Mutex::new(Vec::new()),
                result: result.clone(),
            }
        }

        fn captured_args(&self) -> Vec<Value> {
            self.captured.lock().unwrap().clone()
        }
    }

    impl ShellExecutor for StubShellExecutor {
        fn execute_shell(&self, args: Value) -> ToolResult {
            self.captured.lock().unwrap().push(args);
            self.result.clone()
        }
    }

    #[test]
    fn command_mapping_supports_all_vet_modes() {
        assert_eq!(
            command_for_mode("quick").expect("quick mode should be valid"),
            "just fmt-check && just lint"
        );
        assert_eq!(
            command_for_mode("full").expect("full mode should be valid"),
            "just fmt-check && just lint && just test"
        );
        assert_eq!(
            command_for_mode("cargo-vet").expect("cargo-vet mode should be valid"),
            "cargo vet"
        );
    }

    #[test]
    fn command_mapping_rejects_unknown_mode() {
        let err = command_for_mode("unknown").expect_err("unknown mode should fail");
        assert!(err.contains("quick, full, cargo-vet"));
    }

    #[test]
    fn execute_quick_mode_passes_correct_command() {
        let stub = StubShellExecutor::new(ToolResult::success("ok"));
        let tool = VetTool::with_executor(Box::new(stub));
        let result = tool.execute(json!({"mode": "quick"}));
        assert!(!result.is_error);
    }

    #[test]
    fn execute_full_mode_passes_correct_command() {
        let stub = StubShellExecutor::new(ToolResult::success("tests passed"));
        let tool = VetTool::with_executor(Box::new(stub));
        let result = tool.execute(json!({"mode": "full"}));
        assert!(!result.is_error);
        assert_eq!(result.text_output(), "tests passed");
    }

    #[test]
    fn execute_invalid_mode_returns_error_without_calling_shell() {
        let stub = StubShellExecutor::new(ToolResult::success("should not run"));
        let tool = VetTool::with_executor(Box::new(stub));
        let result = tool.execute(json!({"mode": "bad_mode"}));
        assert!(result.is_error);
        assert!(result.text_output().contains("quick, full, cargo-vet"));
    }

    #[test]
    fn execute_defaults_to_quick_when_mode_missing() {
        let stub = StubShellExecutor::new(ToolResult::success("ok"));
        let tool = VetTool::with_executor(Box::new(stub));
        let result = tool.execute(json!({}));
        assert!(!result.is_error);
    }

    #[test]
    fn execute_passes_timeout_secs_to_shell() {
        let _stub = StubShellExecutor::new(ToolResult::success("ok"));
        let stub_ref = std::sync::Arc::new(StubShellExecutor::new(ToolResult::success("ok")));
        // Can't use Arc with Box<dyn ShellExecutor>, so use a wrapper
        struct ArcStub(std::sync::Arc<StubShellExecutor>);
        impl ShellExecutor for ArcStub {
            fn execute_shell(&self, args: Value) -> ToolResult {
                self.0.execute_shell(args)
            }
        }
        let tool = VetTool::with_executor(Box::new(ArcStub(stub_ref.clone())));
        let _result = tool.execute(json!({"mode": "quick", "timeout_secs": 120}));
        let captured = stub_ref.captured_args();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0]["timeout_secs"], 120);
        assert_eq!(captured[0]["command"], "just fmt-check && just lint");
    }

    #[test]
    fn execute_propagates_shell_failure() {
        let stub = StubShellExecutor::new(ToolResult::failure("lint failed"));
        let tool = VetTool::with_executor(Box::new(stub));
        let result = tool.execute(json!({"mode": "quick"}));
        assert!(result.is_error);
        assert_eq!(result.text_output(), "lint failed");
    }
}
