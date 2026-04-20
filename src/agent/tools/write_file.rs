use super::{Tool, ToolResult, ToolSpec, resolve_workspace_write_path};
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
#[cfg(unix)]
const FINAL_PATH_NOFOLLOW_FLAG: i32 = nix::libc::O_NOFOLLOW;

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

        let (full_path, canonical_root) =
            match resolve_workspace_write_path(&self.workspace_root, path) {
                Ok(values) => values,
                Err(error) => return ToolResult::failure(error),
            };
        if let Some(parent) = full_path.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            return ToolResult::failure(format!(
                "cannot create parent directory {}: {error}",
                parent.display()
            ));
        }
        if let Some(parent) = full_path.parent() {
            match fs::canonicalize(parent) {
                Ok(canonical_parent) if !canonical_parent.starts_with(&canonical_root) => {
                    return ToolResult::failure("path must stay inside workspace root");
                }
                Err(error) => {
                    return ToolResult::failure(format!(
                        "cannot resolve {}: {error}",
                        parent.display()
                    ));
                }
                _ => {}
            }
        }

        match write_utf8_without_following_symlink(&full_path, content) {
            Ok(()) => ToolResult::success(format!("wrote {} bytes to {}", content.len(), path)),
            Err(error) => {
                ToolResult::failure(format!("cannot write {}: {error}", full_path.display()))
            }
        }
    }
}

fn write_utf8_without_following_symlink(
    path: &std::path::Path,
    content: &str,
) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.custom_flags(FINAL_PATH_NOFOLLOW_FLAG);

    let mut file = options.open(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_utf8_without_following_symlink;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(unix)]
    use std::{ffi::OsString, os::unix::fs as unix_fs};

    fn fresh_workspace(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-workspaces")
            .join(format!("blazar-write-file-{label}-{suffix}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn write_utf8_without_following_symlink_rejects_final_symlink_target() {
        let workspace = fresh_workspace("nofollow");
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let outside = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-workspaces")
            .join(OsString::from(format!("write-file-outside-{suffix}")));
        fs::create_dir_all(&outside).unwrap();
        let outside_file = outside.join("secret.txt");
        fs::write(&outside_file, "secret").unwrap();

        let symlink_path = workspace.join("target.txt");
        unix_fs::symlink(&outside_file, &symlink_path).unwrap();

        let error = write_utf8_without_following_symlink(&symlink_path, "updated")
            .expect_err("final symlink target should be rejected");

        assert_eq!(fs::read_to_string(outside_file).unwrap(), "secret");
        assert!(error.kind() != std::io::ErrorKind::NotFound);
    }
}
