use super::{
    ResourceAccess, ResourceClaim, Tool, ToolResult, ToolSpec, normalize_workspace_resource_claim,
    resolve_workspace_path,
};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

const MAX_FILE_BYTES: usize = 100 * 1024;

pub struct ReadFileTool {
    workspace_root: PathBuf,
}

impl ReadFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }
}

impl Tool for ReadFileTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "read_file".into(),
            description: "Read a UTF-8 text file from the workspace. Path is relative to workspace root (e.g. \"src/main.rs\"). Returns the full file content.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to a UTF-8 text file inside the workspace."
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        }
    }

    fn resource_claims(&self, args: &Value) -> Vec<ResourceClaim> {
        args.get("path")
            .and_then(Value::as_str)
            .and_then(|path| normalize_workspace_resource_claim(&self.workspace_root, path).ok())
            .map(|resource| {
                vec![ResourceClaim {
                    resource,
                    access: ResourceAccess::ReadOnly,
                }]
            })
            .unwrap_or_default()
    }

    fn execute(&self, args: Value) -> ToolResult {
        let Some(path) = args.get("path").and_then(|value| value.as_str()) else {
            return ToolResult::failure("read_file requires a string path");
        };

        let full_path = match resolve_workspace_path(&self.workspace_root, path) {
            Ok(path) => path,
            Err(error) => return ToolResult::failure(error),
        };

        match fs::metadata(&full_path) {
            Ok(metadata) if metadata.len() > MAX_FILE_BYTES as u64 => {
                ToolResult::failure("file exceeds 100KB limit")
            }
            Ok(_) => match fs::read(&full_path) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => ToolResult::success(text),
                    Err(error) => ToolResult::failure(format!("file is not valid UTF-8: {error}")),
                },
                Err(error) => {
                    ToolResult::failure(format!("cannot read {}: {error}", full_path.display()))
                }
            },
            Err(error) => {
                ToolResult::failure(format!("cannot read {}: {error}", full_path.display()))
            }
        }
    }
}
