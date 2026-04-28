use super::{
    BuiltinToolDescriptor, BuiltinToolProfiles, ContentPart, Tool, ToolBuildContext, ToolResult,
    ToolSpec, canonical_workspace_root, resolve_workspace_path,
};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 2;

inventory::submit! {
    BuiltinToolDescriptor {
        name: "list_dir",
        profiles: BuiltinToolProfiles::Both,
        build: |ctx: &ToolBuildContext| Box::new(ListDirTool::new(ctx.workspace_root.clone())),
    }
}
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
    ) -> Result<bool, String> {
        if depth > MAX_DEPTH {
            return Ok(false);
        }

        let entries = fs::read_dir(path)
            .map_err(|error| format!("cannot list {}: {error}", path.display()))?;

        let mut entries = entries
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("cannot list {}: {error}", path.display()))?;
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            if lines.len() >= MAX_ENTRIES {
                return Ok(true);
            }

            let file_name = entry.file_name().to_string_lossy().to_string();
            let child_path = entry.path();
            let canonical_child_path = fs::canonicalize(&child_path)
                .map_err(|error| format!("cannot resolve {}: {error}", child_path.display()))?;
            if !canonical_child_path.starts_with(workspace_root) {
                return Err(format!(
                    "cannot list {}: path escapes workspace root",
                    child_path.display()
                ));
            }

            if child_path.is_dir() {
                let rendered = format!("{prefix}{file_name}/");
                lines.push(rendered.clone());

                if depth < MAX_DEPTH
                    && Self::visit(
                        &canonical_child_path,
                        workspace_root,
                        &rendered,
                        depth + 1,
                        lines,
                    )?
                {
                    return Ok(true);
                }
            } else {
                lines.push(format!("{prefix}{file_name}"));
            }
        }

        Ok(false)
    }
}

impl Tool for ListDirTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "list_dir".into(),
            description: "List files and directories in the workspace up to 2 levels deep. Defaults to workspace root. Use to explore project structure before reading or writing files.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative directory path inside the workspace to list."
                    }
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
        let truncated = match Self::visit(&full_path, &canonical_root, "", 0, &mut lines) {
            Ok(truncated) => truncated,
            Err(error) => return ToolResult::failure(error),
        };

        let mut output = lines.join("\n");
        if truncated {
            output.push_str("\n[list truncated]");
        }
        ToolResult {
            content: vec![ContentPart::text(output)],
            exit_code: None,
            is_error: false,
            output_truncated: truncated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fresh_workspace(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-workspaces")
            .join(format!("blazar-list-dir-{label}-{suffix}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn execute_lists_nested_directories() {
        let ws = fresh_workspace("nested");
        fs::create_dir_all(ws.join("src/models")).unwrap();
        fs::write(ws.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(ws.join("src/models/user.rs"), "struct User;").unwrap();
        fs::write(ws.join("README.md"), "# hello").unwrap();

        let tool = ListDirTool::new(ws);
        let result = tool.execute(json!({"path": "."}));

        assert!(!result.is_error);
        let output = result.text_output();
        assert!(output.contains("src/"));
        assert!(output.contains("README.md"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("src/models/"));
    }

    #[test]
    fn execute_handles_empty_directory() {
        let ws = fresh_workspace("empty");
        fs::create_dir_all(ws.join("empty_sub")).unwrap();

        let tool = ListDirTool::new(ws);
        let result = tool.execute(json!({"path": "empty_sub"}));

        assert!(!result.is_error);
        assert!(result.text_output().is_empty());
    }

    #[test]
    fn execute_handles_missing_path() {
        let ws = fresh_workspace("missing");

        let tool = ListDirTool::new(ws);
        let result = tool.execute(json!({"path": "nonexistent"}));

        assert!(result.is_error);
        assert!(result.text_output().contains("cannot resolve"));
    }

    #[test]
    fn execute_rejects_path_escaping_workspace() {
        let ws = fresh_workspace("escape");

        let tool = ListDirTool::new(ws);
        let result = tool.execute(json!({"path": "../.."}));

        assert!(result.is_error);
    }

    #[test]
    fn execute_rejects_non_directory_path() {
        let ws = fresh_workspace("not-dir");
        fs::write(ws.join("file.txt"), "hello").unwrap();

        let tool = ListDirTool::new(ws);
        let result = tool.execute(json!({"path": "file.txt"}));

        assert!(result.is_error);
        assert!(result.text_output().contains("not a directory"));
    }

    #[test]
    fn execute_depth_limit_stops_recursion() {
        let ws = fresh_workspace("depth");
        // Create directories deeper than MAX_DEPTH (2)
        fs::create_dir_all(ws.join("a/b/c/d")).unwrap();
        fs::write(ws.join("a/b/c/d/deep.txt"), "deep").unwrap();

        let tool = ListDirTool::new(ws);
        let result = tool.execute(json!({"path": "."}));

        assert!(!result.is_error);
        let output = result.text_output();
        // Level 0: "a/", Level 1: "a/b/", Level 2: "a/b/c/" — depth=2 is the max
        assert!(output.contains("a/"));
        assert!(output.contains("a/b/"));
        assert!(output.contains("a/b/c/"));
        // "d/" is at depth 3, should not appear
        assert!(!output.contains("d/"));
        assert!(!output.contains("deep.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn execute_rejects_symlink_escaping_workspace() {
        use std::os::unix::fs as unix_fs;

        let ws = fresh_workspace("symlink-escape");
        let outside = fresh_workspace("symlink-outside");
        fs::write(outside.join("secret.txt"), "secret").unwrap();

        unix_fs::symlink(&outside, ws.join("escape_link")).unwrap();

        let tool = ListDirTool::new(ws);
        let result = tool.execute(json!({"path": "."}));

        assert!(result.is_error);
        assert!(result.text_output().contains("escapes workspace root"));
    }

    #[test]
    fn execute_fails_with_invalid_workspace_root() {
        let ws = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-workspaces")
            .join("nonexistent-workspace-root");

        let tool = ListDirTool::new(ws);
        let result = tool.execute(json!({"path": "."}));

        assert!(result.is_error);
        assert!(result.text_output().contains("cannot resolve"));
    }
}
