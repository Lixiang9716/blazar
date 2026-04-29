use super::{
    BuiltinToolDescriptor, BuiltinToolProfiles, ResourceAccess, ResourceClaim, Tool,
    ToolBuildContext, ToolResult, ToolSpec, normalize_workspace_resource_claim,
    resolve_workspace_path,
};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

inventory::submit! {
    BuiltinToolDescriptor {
        name: "read_file",
        profiles: BuiltinToolProfiles::Both,
        build: |ctx: &ToolBuildContext| Box::new(ReadFileTool::new(ctx.workspace_root.clone())),
    }
}

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
            Ok(metadata) if metadata.len() > MAX_FILE_BYTES => ToolResult::failure(format!(
                "file is too large to read: limit is {MAX_FILE_BYTES} bytes"
            )),
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
            .join(format!("blazar-read-file-{label}-{suffix}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn execute_reads_utf8_file_successfully() {
        let ws = fresh_workspace("happy");
        fs::write(ws.join("hello.txt"), "Hello, world!").unwrap();

        let tool = ReadFileTool::new(ws);
        let result = tool.execute(json!({"path": "hello.txt"}));

        assert!(!result.is_error);
        assert_eq!(result.text_output(), "Hello, world!");
    }

    #[test]
    fn execute_returns_error_for_missing_file() {
        let ws = fresh_workspace("missing");

        let tool = ReadFileTool::new(ws);
        let result = tool.execute(json!({"path": "nonexistent.txt"}));

        assert!(result.is_error);
        assert!(result.text_output().contains("cannot resolve"));
    }

    #[test]
    fn execute_rejects_path_escaping_workspace() {
        let ws = fresh_workspace("escape");

        let tool = ReadFileTool::new(ws);
        let result = tool.execute(json!({"path": "../../etc/passwd"}));

        assert!(result.is_error);
    }

    #[test]
    fn execute_rejects_file_exceeding_size_limit() {
        let ws = fresh_workspace("large");
        let file_path = ws.join("large.txt");
        let file = fs::File::create(&file_path).unwrap();
        file.set_len(MAX_FILE_BYTES + 1).unwrap();

        let tool = ReadFileTool::new(ws);
        let result = tool.execute(json!({"path": "large.txt"}));

        assert!(result.is_error);
        assert!(
            result
                .text_output()
                .contains(&format!("limit is {} bytes", MAX_FILE_BYTES))
        );
    }

    #[test]
    fn execute_rejects_non_utf8_file() {
        let ws = fresh_workspace("binary");
        // Write invalid UTF-8 bytes
        fs::write(ws.join("binary.bin"), [0xFF, 0xFE, 0x80, 0x81]).unwrap();

        let tool = ReadFileTool::new(ws);
        let result = tool.execute(json!({"path": "binary.bin"}));

        assert!(result.is_error);
        assert!(result.text_output().contains("not valid UTF-8"));
    }

    #[test]
    fn execute_returns_error_for_missing_path_param() {
        let ws = fresh_workspace("no-param");

        let tool = ReadFileTool::new(ws);
        let result = tool.execute(json!({}));

        assert!(result.is_error);
        assert!(result.text_output().contains("requires a string path"));
    }
}
