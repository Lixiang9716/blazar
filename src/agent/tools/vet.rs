use std::path::PathBuf;

use serde_json::{Value, json};

use super::bash::BashTool;
use super::{ResourceAccess, ResourceClaim, Tool, ToolResult, ToolSpec};

pub struct VetTool {
    workspace_root: PathBuf,
}

impl VetTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
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

        BashTool::new(self.workspace_root.clone()).execute(shell_args)
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
    use super::command_for_mode;

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
}
