use super::{
    Tool, ToolResult, ToolSpec, canonical_workspace_root, resolve_workspace_path,
};
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

    fn visit(
        path: &Path,
        workspace_root: &Path,
        prefix: &str,
        depth: usize,
        lines: &mut Vec<String>,
    ) -> bool {
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
            let Ok(canonical_child_path) = fs::canonicalize(&child_path) else {
                continue;
            };
            if !canonical_child_path.starts_with(workspace_root) {
                continue;
            }

            if child_path.is_dir() {
                let rendered = format!("{prefix}{file_name}/");
                lines.push(rendered.clone());

                if Self::visit(
                    &canonical_child_path,
                    workspace_root,
                    &rendered,
                    depth + 1,
                    lines,
                ) {
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
        let Some(path) = args.get("path").and_then(|value| value.as_str()) else {
            return ToolResult::failure("list_dir requires a string path");
        };

        let canonical_root = match canonical_workspace_root(&self.workspace_root) {
            Ok(root) => root,
            Err(error) => return ToolResult::failure(error),
        };
        let full_path = match resolve_workspace_path(&self.workspace_root, path) {
            Ok(path) => path,
            Err(error) => return ToolResult::failure(error),
        };
        if !full_path.is_dir() {
            return ToolResult::failure(format!(
                "cannot list {}: not a directory",
                full_path.display()
            ));
        }

        let mut lines = Vec::new();
        let truncated = Self::visit(&full_path, &canonical_root, "", 0, &mut lines);

        let mut output = lines.join("\n");
        if truncated {
            output.push_str("\n[list truncated]");
        }
        ToolResult {
            output,
            exit_code: None,
            is_error: false,
            output_truncated: truncated,
        }
    }
}
