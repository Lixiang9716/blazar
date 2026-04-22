pub mod acp;
pub mod agent;
pub mod bash;
pub mod list_dir;
pub mod read_file;
pub mod scheduler;
pub mod write_file;

use serde_json::Value;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Declarative metadata advertised to the model for each callable tool.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentProtocol {
    Native,
    Acp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Local,
    Agent { protocol: AgentProtocol },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceAccess {
    ReadOnly,
    ReadWrite,
    Exclusive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceClaim {
    pub resource: String,
    pub access: ResourceAccess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentPart {
    Text {
        text: String,
    },
    Resource {
        uri: String,
        mime_type: Option<String>,
    },
}

impl ContentPart {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }
}

/// Canonical tool execution payload returned to the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    pub content: Vec<ContentPart>,
    pub exit_code: Option<i32>,
    pub is_error: bool,
    pub output_truncated: bool,
}

impl ToolResult {
    /// Builds a successful tool result with plain-text output.
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            content: vec![ContentPart::text(output)],
            exit_code: None,
            is_error: false,
            output_truncated: false,
        }
    }

    /// Builds an error tool result without attaching an OS exit code.
    pub fn failure(output: impl Into<String>) -> Self {
        Self {
            content: vec![ContentPart::text(output)],
            exit_code: None,
            is_error: true,
            output_truncated: false,
        }
    }

    pub fn text_output(&self) -> String {
        self.content
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text { text } => Some(text.as_str()),
                ContentPart::Resource { .. } => None,
            })
            .collect::<String>()
    }
}

/// Trait implemented by all model-callable tools.
///
/// `execute` must be deterministic for identical inputs and should report
/// user-facing validation errors in [`ToolResult::failure`].
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn kind(&self) -> ToolKind {
        ToolKind::Local
    }
    fn resource_claims(&self, _args: &Value) -> Vec<ResourceClaim> {
        Vec::new()
    }
    fn execute(&self, args: Value) -> ToolResult;
}

/// Runtime registry that owns tool implementations for a workspace.
pub struct ToolRegistry {
    workspace_root: PathBuf,
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    /// Creates an empty registry scoped to `workspace_root`.
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            tools: Vec::new(),
        }
    }

    /// Returns the configured workspace root used by path-aware tools.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Adds a tool implementation to the registry.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    /// Looks up a tool by its advertised `ToolSpec::name`.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|tool| tool.spec().name == name)
            .map(|tool| tool.as_ref())
    }

    /// Returns the public specs for all registered tools.
    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.iter().map(|tool| tool.spec()).collect()
    }

    /// Executes a named tool and returns an error when the tool is unknown.
    pub fn execute(&self, name: &str, args: Value) -> Result<ToolResult, String> {
        match self.get(name) {
            Some(tool) => Ok(tool.execute(args)),
            None => Err(format!("unknown tool: {name}")),
        }
    }
}

/// Validates a user-provided path is workspace-relative and traversal-safe.
///
/// This rejects absolute paths and components that can escape the workspace
/// (`..`, root markers, and Windows prefixes).
pub fn validate_workspace_relative_path(requested: &str) -> Result<(), String> {
    let path = Path::new(requested);
    if path.is_absolute() {
        return Err("absolute paths are not allowed".into());
    }

    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err("path must stay inside workspace root".into());
        }
    }

    Ok(())
}

/// Canonicalizes `workspace_root` and emits a context-rich error on failure.
pub fn canonical_workspace_root(workspace_root: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(workspace_root).map_err(|error| {
        format!(
            "cannot resolve workspace root {}: {error}",
            workspace_root.display()
        )
    })
}

fn ensure_path_is_within_workspace(path: &Path, workspace_root: &Path) -> Result<(), String> {
    if path.starts_with(workspace_root) {
        Ok(())
    } else {
        Err("path must stay inside workspace root".into())
    }
}

fn canonicalize_existing_ancestor(path: &Path) -> Result<PathBuf, String> {
    for ancestor in path.ancestors() {
        if ancestor.exists() {
            return fs::canonicalize(ancestor)
                .map_err(|error| format!("cannot resolve {}: {error}", ancestor.display()));
        }
    }

    Err(format!(
        "cannot resolve {}: no existing ancestor",
        path.display()
    ))
}

/// Resolves a read path inside the workspace and returns its canonical path.
///
/// The target must already exist and remain under `workspace_root`.
pub fn resolve_workspace_path(workspace_root: &Path, requested: &str) -> Result<PathBuf, String> {
    validate_workspace_relative_path(requested)?;

    let canonical_root = canonical_workspace_root(workspace_root)?;
    let canonical_path = fs::canonicalize(workspace_root.join(requested)).map_err(|error| {
        format!(
            "cannot resolve {}: {error}",
            workspace_root.join(requested).display()
        )
    })?;
    ensure_path_is_within_workspace(&canonical_path, &canonical_root)?;
    Ok(canonical_path)
}

/// Resolves a safe write target relative to `workspace_root`.
///
/// Returns:
/// - the unresolved full write path (preserving caller intent), and
/// - the canonical workspace root used for containment checks.
///
/// Existing symlink targets are rejected to avoid writing through links.
pub fn resolve_workspace_write_path(
    workspace_root: &Path,
    requested: &str,
) -> Result<(PathBuf, PathBuf), String> {
    validate_workspace_relative_path(requested)?;

    let canonical_root = canonical_workspace_root(workspace_root)?;
    let full_path = workspace_root.join(requested);
    let parent = full_path.parent().unwrap_or(workspace_root);

    let canonical_parent = canonicalize_existing_ancestor(parent)?;
    ensure_path_is_within_workspace(&canonical_parent, &canonical_root)?;

    if let Ok(metadata) = fs::symlink_metadata(&full_path) {
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "cannot write {}: target path is a symlink",
                full_path.display()
            ));
        }

        let canonical_path = fs::canonicalize(&full_path)
            .map_err(|error| format!("cannot resolve {}: {error}", full_path.display()))?;
        ensure_path_is_within_workspace(&canonical_path, &canonical_root)?;
    }

    Ok((full_path, canonical_root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::echo::EchoProvider;
    use serde_json::json;
    use std::sync::Arc;

    struct EchoTool;

    impl Tool for EchoTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "echo".into(),
                description: "echo tool".into(),
                parameters: json!({"type":"object"}),
            }
        }

        fn execute(&self, args: Value) -> ToolResult {
            ToolResult::success(args.to_string())
        }
    }

    fn unique_workspace(name: &str) -> PathBuf {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
        std::fs::create_dir_all(&base).expect("target dir should exist");
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        base.join(format!("tools-{name}-{suffix}"))
    }

    #[test]
    fn tool_result_helpers_build_expected_flags() {
        let ok = ToolResult::success("done");
        assert_eq!(
            ok.content,
            vec![ContentPart::Text {
                text: "done".into()
            }]
        );
        assert_eq!(ok.text_output(), "done");
        assert!(!ok.is_error);
        assert_eq!(ok.exit_code, None);

        let err = ToolResult::failure("nope");
        assert_eq!(
            err.content,
            vec![ContentPart::Text {
                text: "nope".into()
            }]
        );
        assert_eq!(err.text_output(), "nope");
        assert!(err.is_error);
        assert!(!err.output_truncated);
    }

    #[test]
    fn registry_registers_specs_and_executes_known_tools() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut registry = ToolRegistry::new(root.clone());
        registry.register(Box::new(EchoTool));

        assert_eq!(registry.workspace_root(), root.as_path());
        assert!(registry.get("echo").is_some());
        assert!(registry.get("missing").is_none());
        assert_eq!(registry.specs().len(), 1);

        let result = registry
            .execute("echo", json!({"value":"x"}))
            .expect("tool should execute");
        assert_eq!(result.text_output(), r#"{"value":"x"}"#);
        assert_eq!(
            registry.execute("missing", json!({})).expect_err("unknown"),
            "unknown tool: missing"
        );
    }

    #[test]
    fn tool_defaults_report_local_kind_and_no_resource_claims() {
        let tool = EchoTool;

        assert_eq!(tool.kind(), ToolKind::Local);
        assert!(tool.resource_claims(&json!({"value":"x"})).is_empty());
    }

    #[test]
    fn tool_result_text_output_ignores_non_text_content_parts() {
        let result = ToolResult {
            content: vec![ContentPart::Resource {
                uri: "file://workspace/out.txt".into(),
                mime_type: Some("text/plain".into()),
            }],
            exit_code: None,
            is_error: false,
            output_truncated: false,
        };

        assert_eq!(result.text_output(), "");
    }

    #[test]
    fn specialized_tools_report_expected_kind_and_resource_claims() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let read_tool = read_file::ReadFileTool::new(workspace_root.clone());
        let write_tool = write_file::WriteFileTool::new(workspace_root.clone());
        let bash_tool = bash::BashTool::new(workspace_root.clone());
        let agent_tool = agent::AgentTool::new(
            "delegate",
            "delegate work",
            Arc::new(EchoProvider::new(0)),
            "echo",
            workspace_root,
        );

        assert_eq!(
            read_tool.resource_claims(&json!({"path":"src/main.rs"})),
            vec![ResourceClaim {
                resource: "fs:src/main.rs".into(),
                access: ResourceAccess::ReadOnly,
            }]
        );
        assert_eq!(
            write_tool.resource_claims(&json!({"path":"src/main.rs"})),
            vec![ResourceClaim {
                resource: "fs:src/main.rs".into(),
                access: ResourceAccess::ReadWrite,
            }]
        );
        assert_eq!(
            bash_tool.resource_claims(&json!({"command":"pwd"})),
            vec![ResourceClaim {
                resource: "process:bash".into(),
                access: ResourceAccess::Exclusive,
            }]
        );
        assert_eq!(
            agent_tool.kind(),
            ToolKind::Agent {
                protocol: AgentProtocol::Native,
            }
        );
    }

    #[test]
    fn path_validation_rejects_absolute_and_parent_components() {
        assert!(validate_workspace_relative_path("src/main.rs").is_ok());
        assert!(validate_workspace_relative_path("/etc/passwd").is_err());
        assert!(validate_workspace_relative_path("../secret").is_err());
        assert!(validate_workspace_relative_path("nested/../../oops").is_err());
    }

    #[test]
    fn resolve_workspace_path_returns_canonical_path_inside_workspace() {
        let workspace = unique_workspace("resolve-read");
        std::fs::create_dir_all(workspace.join("src")).expect("create workspace");
        std::fs::write(workspace.join("src/file.txt"), "hello").expect("write test file");

        let resolved = resolve_workspace_path(&workspace, "src/file.txt").expect("resolve path");
        assert!(resolved.ends_with("src/file.txt"));

        let err = resolve_workspace_path(&workspace, "../escape")
            .expect_err("path traversal must be rejected");
        assert!(err.contains("path must stay inside workspace root"));

        let missing = resolve_workspace_path(&workspace, "src/missing.txt")
            .expect_err("missing file should fail");
        assert!(missing.contains("cannot resolve"));

        std::fs::remove_dir_all(workspace).expect("cleanup workspace");
    }

    #[test]
    fn resolve_workspace_write_path_rejects_symlink_targets() {
        let workspace = unique_workspace("resolve-write");
        std::fs::create_dir_all(workspace.join("dir")).expect("create workspace");
        std::fs::write(workspace.join("dir/real.txt"), "real").expect("write file");

        let (full_path, canonical_root) =
            resolve_workspace_write_path(&workspace, "dir/new.txt").expect("new write path");
        assert!(full_path.ends_with("dir/new.txt"));
        assert_eq!(
            canonical_root,
            std::fs::canonicalize(&workspace).expect("canonical root")
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(
                workspace.join("dir/real.txt"),
                workspace.join("dir/link.txt"),
            )
            .expect("create symlink");
            let err = resolve_workspace_write_path(&workspace, "dir/link.txt")
                .expect_err("symlink writes should fail");
            assert!(err.contains("target path is a symlink"));
        }

        let err = resolve_workspace_write_path(&workspace, "../outside.txt")
            .expect_err("traversal should fail");
        assert!(err.contains("path must stay inside workspace root"));

        std::fs::remove_dir_all(workspace).expect("cleanup workspace");
    }

    #[test]
    fn canonical_workspace_root_reports_missing_directories() {
        let missing = unique_workspace("missing");
        let err = canonical_workspace_root(&missing).expect_err("missing dir should fail");
        assert!(err.contains("cannot resolve workspace root"));
    }
}
