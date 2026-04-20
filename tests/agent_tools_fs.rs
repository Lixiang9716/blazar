use blazar::agent::tools::Tool;
use blazar::agent::tools::list_dir::ListDirTool;
use blazar::agent::tools::read_file::ReadFileTool;
use blazar::agent::tools::write_file::WriteFileTool;
use serde_json::json;
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
        .join(format!("blazar-{label}-{suffix}"));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn read_file_reads_workspace_relative_path() {
    let workspace = fresh_workspace("read-file");
    fs::write(workspace.join("notes.txt"), "hello tool system").unwrap();

    let tool = ReadFileTool::new(workspace.clone());
    let result = tool.execute(json!({ "path": "notes.txt" }));

    assert_eq!(result.output, "hello tool system");
    assert!(!result.is_error);
}

#[test]
fn read_file_reads_utf8_up_to_100kb() {
    let workspace = fresh_workspace("read-limit");
    let content = "a".repeat(100 * 1024);
    fs::write(workspace.join("limit.txt"), &content).unwrap();

    let tool = ReadFileTool::new(workspace.clone());
    let result = tool.execute(json!({ "path": "limit.txt" }));

    assert!(!result.is_error);
    assert_eq!(result.output.len(), 100 * 1024);
}

#[test]
fn read_file_rejects_files_over_100kb() {
    let workspace = fresh_workspace("read-too-large");
    let content = "a".repeat(100 * 1024 + 1);
    fs::write(workspace.join("too-large.txt"), &content).unwrap();

    let tool = ReadFileTool::new(workspace.clone());
    let result = tool.execute(json!({ "path": "too-large.txt" }));

    assert!(result.is_error);
    assert_eq!(result.output, "file exceeds 100KB limit");
}

#[test]
fn write_file_creates_missing_parent_directories() {
    let workspace = fresh_workspace("write-file");
    let tool = WriteFileTool::new(workspace.clone());

    let result = tool.execute(json!({
        "path": "nested/output.txt",
        "content": "written by tool"
    }));

    assert!(!result.is_error);
    assert_eq!(result.output, "wrote 15 bytes to nested/output.txt");
    assert_eq!(
        fs::read_to_string(workspace.join("nested/output.txt")).unwrap(),
        "written by tool"
    );
}

#[test]
fn list_dir_stops_after_two_levels() {
    let workspace = fresh_workspace("list-dir");
    fs::create_dir_all(workspace.join("a/b/c")).unwrap();
    fs::write(workspace.join("a/root.txt"), "root").unwrap();
    fs::write(workspace.join("a/b/inner.txt"), "inner").unwrap();
    fs::write(workspace.join("a/b/c/deep.txt"), "deep").unwrap();

    let tool = ListDirTool::new(workspace.clone());
    let result = tool.execute(json!({ "path": "." }));

    assert!(!result.is_error);
    assert!(result.output.contains("a/"));
    assert!(result.output.contains("a/b/"));
    assert!(!result.output.contains("a/b/inner.txt"));
    assert!(!result.output.contains("a/b/c/"));
    assert!(!result.output.contains("a/b/c/deep.txt"));
}

#[test]
fn list_dir_stops_after_two_hundred_entries() {
    let workspace = fresh_workspace("list-truncate");
    for index in 0..205 {
        fs::write(workspace.join(format!("file-{index:03}.txt")), "x").unwrap();
    }

    let tool = ListDirTool::new(workspace.clone());
    let result = tool.execute(json!({ "path": "." }));

    assert!(!result.is_error);
    assert!(result.output.ends_with("[list truncated]"));
    assert!(result.output_truncated);
    assert_eq!(result.output.lines().count(), 201);
    assert!(!result.output.contains("file-200.txt"));
}

#[test]
fn list_dir_does_not_truncate_exactly_two_hundred_entries() {
    let workspace = fresh_workspace("list-exact-limit");
    for index in 0..200 {
        fs::write(workspace.join(format!("file-{index:03}.txt")), "x").unwrap();
    }

    let tool = ListDirTool::new(workspace.clone());
    let result = tool.execute(json!({ "path": "." }));

    assert!(!result.is_error);
    assert!(!result.output.ends_with("[list truncated]"));
    assert!(!result.output_truncated);
    assert_eq!(result.output.lines().count(), 200);
}

#[test]
fn list_dir_requires_path_argument() {
    let workspace = fresh_workspace("list-missing-path");
    let tool = ListDirTool::new(workspace.clone());

    let result = tool.execute(json!({}));

    assert!(result.is_error);
}

#[test]
fn file_tools_reject_parent_escape() {
    let workspace = fresh_workspace("escape");
    let read_tool = ReadFileTool::new(workspace.clone());
    let write_tool = WriteFileTool::new(workspace.clone());
    let list_tool = ListDirTool::new(workspace.clone());

    let read = read_tool.execute(json!({ "path": "../secret.txt" }));
    let write = write_tool.execute(json!({
        "path": "../secret.txt",
        "content": "nope"
    }));
    let list = list_tool.execute(json!({ "path": "../secret.txt" }));

    assert!(read.is_error);
    assert!(write.is_error);
    assert!(list.is_error);
}

#[cfg(unix)]
#[test]
fn file_tools_reject_symlink_escapes() {
    let workspace = fresh_workspace("symlink-escape");
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let outside = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-workspaces")
        .join(OsString::from(format!("outside-{suffix}")));
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secret.txt"), "secret").unwrap();
    unix_fs::symlink(&outside, workspace.join("escape-dir")).unwrap();
    unix_fs::symlink(outside.join("secret.txt"), workspace.join("escape-file.txt")).unwrap();

    let read_tool = ReadFileTool::new(workspace.clone());
    let write_tool = WriteFileTool::new(workspace.clone());
    let list_tool = ListDirTool::new(workspace.clone());

    let read = read_tool.execute(json!({ "path": "escape-file.txt" }));
    let write = write_tool.execute(json!({
        "path": "escape-dir/new.txt",
        "content": "nope"
    }));
    let list = list_tool.execute(json!({ "path": "escape-dir" }));

    assert!(read.is_error);
    assert!(write.is_error);
    assert!(list.is_error);
    assert!(!outside.join("new.txt").exists());
}

#[cfg(unix)]
#[test]
fn list_dir_fails_when_traversal_hits_a_broken_symlink() {
    let workspace = fresh_workspace("list-broken-symlink");
    fs::create_dir_all(workspace.join("dir")).unwrap();
    unix_fs::symlink(workspace.join("missing-target"), workspace.join("dir/broken-link")).unwrap();

    let tool = ListDirTool::new(workspace.clone());
    let result = tool.execute(json!({ "path": "dir" }));

    assert!(result.is_error);
}
