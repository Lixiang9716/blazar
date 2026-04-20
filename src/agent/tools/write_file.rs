use super::{Tool, ToolResult, ToolSpec, validate_workspace_relative_path};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

pub struct WriteFileTool {
    workspace_root: PathBuf,
}

impl WriteFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }
}

impl Tool for WriteFileTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "write_file".into(),
            description: "Write a UTF-8 file inside the workspace.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        }
    }

    fn execute(&self, args: Value) -> ToolResult {
        let Some(path) = args.get("path").and_then(|value| value.as_str()) else {
            return ToolResult::failure("write_file requires a string path");
        };
        let Some(content) = args.get("content").and_then(|value| value.as_str()) else {
            return ToolResult::failure("write_file requires string content");
        };

        if let Err(error) = validate_workspace_relative_path(path) {
            return ToolResult::failure(error);
        }

        let full_path = self.workspace_root.join(path);
        if let Some(parent) = full_path.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            return ToolResult::failure(format!(
                "cannot create parent directory {}: {error}",
                parent.display()
            ));
        }

        match fs::write(&full_path, content) {
            Ok(()) => ToolResult::success(format!("wrote {} bytes to {}", content.len(), path)),
            Err(error) => {
                ToolResult::failure(format!("cannot write {}: {error}", full_path.display()))
            }
        }
    }
}
