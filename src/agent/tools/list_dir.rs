use super::{Tool, ToolResult, ToolSpec, validate_workspace_relative_path};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 2;
const MAX_ENTRIES: usize = 200;

pub struct ListDirTool {
    workspace_root: PathBuf,
}

impl ListDirTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    fn visit(path: &Path, prefix: &str, depth: usize, lines: &mut Vec<String>) -> bool {
        if depth >= MAX_DEPTH {
            return false;
        }

        let Ok(entries) = fs::read_dir(path) else {
            return false;
        };

        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            if lines.len() >= MAX_ENTRIES {
                return true;
            }

            let file_name = entry.file_name().to_string_lossy().to_string();
            let child_path = entry.path();
            if child_path.is_dir() {
                let rendered = format!("{prefix}{file_name}/");
                lines.push(rendered.clone());

                if Self::visit(&child_path, &rendered, depth + 1, lines) {
                    return true;
                }
            } else {
                lines.push(format!("{prefix}{file_name}"));
            }
        }

        false
    }
}

impl Tool for ListDirTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "list_dir".into(),
            description: "List workspace files and directories up to two levels deep.".into(),
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
        let path = args
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or(".");

        if let Err(error) = validate_workspace_relative_path(path) {
            return ToolResult::failure(error);
        }

        let full_path = self.workspace_root.join(path);
        if !full_path.is_dir() {
            return ToolResult::failure(format!(
                "cannot list {}: not a directory",
                full_path.display()
            ));
        }

        let mut lines = Vec::new();
        let truncated = Self::visit(&full_path, "", 0, &mut lines);

        let mut output = lines.join("\n");
        if truncated {
            output.push_str("\n[list truncated]");
        }
        ToolResult::success(output)
    }
}
