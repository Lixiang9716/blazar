use super::{Tool, ToolResult, ToolSpec, validate_workspace_relative_path};
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
            description: "Read a UTF-8 file from the workspace.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        }
    }

    fn execute(&self, args: Value) -> ToolResult {
        let Some(path) = args.get("path").and_then(|value| value.as_str()) else {
            return ToolResult::failure("read_file requires a string path");
        };

        if let Err(error) = validate_workspace_relative_path(path) {
            return ToolResult::failure(error);
        }

        let full_path = self.workspace_root.join(path);
        match fs::read(&full_path) {
            Ok(bytes) if bytes.len() > MAX_FILE_BYTES => {
                ToolResult::failure("file exceeds 100KB limit")
            }
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => ToolResult::success(text),
                Err(error) => ToolResult::failure(format!("file is not valid UTF-8: {error}")),
            },
            Err(error) => {
                ToolResult::failure(format!("cannot read {}: {error}", full_path.display()))
            }
        }
    }
}
