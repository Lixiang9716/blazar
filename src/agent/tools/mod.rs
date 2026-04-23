pub mod acp;
pub mod agent;
pub mod bash;
pub mod list_dir;
pub mod read_file;
#[allow(dead_code)]
pub mod scheduler;
pub mod write_file;

use serde_json::Value;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::agent::capability::local::LocalToolCapability;
use crate::agent::capability::{
    CapabilityAccess, CapabilityClaim, CapabilityContentPart, CapabilityError, CapabilityInput,
    CapabilityKind, CapabilityResult,
};

/// Declarative metadata advertised to the model for each callable tool.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Local,
    Agent { is_acp: bool },
}

impl ToolKind {
    pub fn into_capability_kind(self) -> CapabilityKind {
        self.into()
    }
}

impl From<ToolKind> for CapabilityKind {
    fn from(value: ToolKind) -> Self {
        match value {
            ToolKind::Local => Self::Local,
            ToolKind::Agent { is_acp } => Self::Agent { is_acp },
        }
    }
}

impl From<CapabilityKind> for ToolKind {
    fn from(value: CapabilityKind) -> Self {
        match value {
            CapabilityKind::Local => Self::Local,
            CapabilityKind::Agent { is_acp } => Self::Agent { is_acp },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceAccess {
    ReadOnly,
    ReadWrite,
    Exclusive,
}

impl From<ResourceAccess> for CapabilityAccess {
    fn from(value: ResourceAccess) -> Self {
        match value {
            ResourceAccess::ReadOnly => Self::ReadOnly,
            ResourceAccess::ReadWrite => Self::ReadWrite,
            ResourceAccess::Exclusive => Self::Exclusive,
        }
    }
}

impl From<CapabilityAccess> for ResourceAccess {
    fn from(value: CapabilityAccess) -> Self {
        match value {
            CapabilityAccess::ReadOnly => Self::ReadOnly,
            CapabilityAccess::ReadWrite => Self::ReadWrite,
            CapabilityAccess::Exclusive => Self::Exclusive,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceClaim {
    pub resource: String,
    pub access: ResourceAccess,
}

impl ResourceClaim {
    pub fn into_capability_claim(self) -> CapabilityClaim {
        self.into()
    }
}

impl From<ResourceClaim> for CapabilityClaim {
    fn from(value: ResourceClaim) -> Self {
        Self {
            resource: value.resource,
            access: value.access.into(),
        }
    }
}

impl From<CapabilityClaim> for ResourceClaim {
    fn from(value: CapabilityClaim) -> Self {
        Self {
            resource: value.resource,
            access: value.access.into(),
        }
    }
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

    fn text_projection(&self) -> String {
        match self {
            Self::Text { text } => text.clone(),
            Self::Resource { uri, mime_type } => match mime_type {
                Some(mime_type) => format!("[resource] {uri} ({mime_type})"),
                None => format!("[resource] {uri}"),
            },
        }
    }
}

impl From<ContentPart> for CapabilityContentPart {
    fn from(value: ContentPart) -> Self {
        match value {
            ContentPart::Text { text } => Self::Text { text },
            ContentPart::Resource { uri, mime_type } => Self::Resource { uri, mime_type },
        }
    }
}

impl From<CapabilityContentPart> for ContentPart {
    fn from(value: CapabilityContentPart) -> Self {
        match value {
            CapabilityContentPart::Text { text } => Self::Text { text },
            CapabilityContentPart::Resource { uri, mime_type } => Self::Resource { uri, mime_type },
        }
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
        // Keep this projection behavior in lockstep with CapabilityResult::text_output.
        let mut output = String::new();
        let mut previous_was_resource = false;
        for part in &self.content {
            match part {
                ContentPart::Text { text } => {
                    if previous_was_resource && !output.ends_with('\n') && !text.starts_with('\n') {
                        output.push('\n');
                    }
                    output.push_str(text);
                    previous_was_resource = false;
                }
                ContentPart::Resource { .. } => {
                    if !output.is_empty() && !output.ends_with('\n') {
                        output.push('\n');
                    }
                    output.push_str(&part.text_projection());
                    previous_was_resource = true;
                }
            }
        }
        output
    }

    pub fn into_capability_result(self) -> CapabilityResult {
        self.into()
    }

    pub fn from_capability_result(result: CapabilityResult) -> Self {
        result.into()
    }
}

impl From<ToolResult> for CapabilityResult {
    fn from(value: ToolResult) -> Self {
        let error = if value.is_error {
            Some(CapabilityError::new(value.text_output()))
        } else {
            None
        };

        Self {
            content: value.content.into_iter().map(Into::into).collect(),
            exit_code: value.exit_code,
            is_error: value.is_error,
            output_truncated: value.output_truncated,
            error,
        }
    }
}

impl From<CapabilityResult> for ToolResult {
    fn from(value: CapabilityResult) -> Self {
        let CapabilityResult {
            content,
            exit_code,
            is_error,
            output_truncated,
            error,
        } = value;

        let mut projected_content = content.into_iter().map(Into::into).collect::<Vec<_>>();
        if projected_content.is_empty()
            && let Some(error) = &error
        {
            projected_content.push(ContentPart::text(error.message.clone()));
        }

        Self {
            content: projected_content,
            exit_code,
            is_error: is_error || error.is_some(),
            output_truncated,
        }
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

    pub fn contains_name(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Returns the public specs for all registered tools.
    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.iter().map(|tool| tool.spec()).collect()
    }

    pub fn resource_claims(&self, name: &str, args: &Value) -> Vec<ResourceClaim> {
        let Some(tool) = self.get(name) else {
            return Vec::new();
        };

        if should_use_local_capability_wrapper(tool.kind()) {
            let capability = LocalToolCapability::from_tool(tool);
            capability.resource_claims(&CapabilityInput::new(args.clone()))
        } else {
            tool.resource_claims(args)
        }
    }

    /// Executes a named tool and returns an error when the tool is unknown.
    pub fn execute(&self, name: &str, args: Value) -> Result<ToolResult, String> {
        match self.get(name) {
            Some(tool) => {
                if should_use_local_capability_wrapper(tool.kind()) {
                    let capability = LocalToolCapability::from_tool(tool);
                    let result = capability.execute(CapabilityInput::new(args));
                    Ok(ToolResult::from_capability_result(result))
                } else {
                    Ok(tool.execute(args))
                }
            }
            None => Err(format!("unknown tool: {name}")),
        }
    }
}

fn should_use_local_capability_wrapper(kind: ToolKind) -> bool {
    // Keep local capability wrappers on built-in local tooling (including
    // delegated non-ACP agent turns), while ACP-backed agents stay on the
    // protocol adapter path.
    matches!(kind, ToolKind::Local | ToolKind::Agent { is_acp: false })
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

/// Builds a normalized scheduler resource id for a workspace-relative path.
///
/// Equivalent lexical paths such as `src/lib.rs` and `./src/lib.rs` collapse to
/// the same resource key after validation. This is a lexical normalization only;
/// it does not resolve symlink aliases within the workspace.
pub fn normalize_workspace_resource_claim(
    _workspace_root: &Path,
    requested: &str,
) -> Result<String, String> {
    validate_workspace_relative_path(requested)?;

    let normalized = Path::new(requested)
        .components()
        .filter_map(|component| match component {
            Component::CurDir => None,
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");

    if normalized.is_empty() {
        return Err("path must include a file name".into());
    }

    Ok(format!("fs:{normalized}"))
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
    use crate::agent::capability::local::LocalToolCapability;
    use crate::agent::capability::{
        CapabilityAccess, CapabilityClaim, CapabilityError, CapabilityInput, CapabilityKind,
        CapabilityResult, ConflictPolicy,
    };
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
    fn tool_result_round_trips_through_capability_result() {
        let original = ToolResult {
            content: vec![
                ContentPart::text("summary"),
                ContentPart::Resource {
                    uri: "file://workspace/out.txt".into(),
                    mime_type: Some("text/plain".into()),
                },
                ContentPart::text("details"),
            ],
            exit_code: Some(0),
            is_error: false,
            output_truncated: true,
        };

        let converted = original.clone().into_capability_result();
        let round_tripped = ToolResult::from_capability_result(converted);
        assert_eq!(round_tripped, original);
        assert_eq!(round_tripped.text_output(), original.text_output());
    }

    #[test]
    fn capability_result_error_metadata_stays_behavior_compatible() {
        let capability_result = CapabilityResult {
            content: Vec::new(),
            exit_code: None,
            is_error: false,
            output_truncated: false,
            error: Some(CapabilityError::with_code("ACP_TIMEOUT", "timed out")),
        };

        let tool_result: ToolResult = capability_result.into();
        assert!(tool_result.is_error);
        assert_eq!(tool_result.text_output(), "timed out");
    }

    #[test]
    fn tool_kind_and_claims_convert_to_capability_contracts() {
        assert_eq!(
            CapabilityKind::from(ToolKind::Agent { is_acp: true }),
            CapabilityKind::Agent { is_acp: true }
        );
        assert_eq!(
            ToolKind::from(CapabilityKind::Agent { is_acp: false }),
            ToolKind::Agent { is_acp: false }
        );

        let claim = ResourceClaim {
            resource: "fs:src/main.rs".into(),
            access: ResourceAccess::ReadWrite,
        };
        let capability_claim: CapabilityClaim = claim.clone().into_capability_claim();
        assert_eq!(
            capability_claim,
            CapabilityClaim {
                resource: "fs:src/main.rs".into(),
                access: CapabilityAccess::ReadWrite,
            }
        );
        assert_eq!(ResourceClaim::from(capability_claim), claim);
    }

    #[test]
    fn mixed_content_projection_stays_in_lockstep_across_tool_and_capability_results() {
        let tool_result = ToolResult {
            content: vec![
                ContentPart::text("summary"),
                ContentPart::Resource {
                    uri: "file://workspace/out.txt".into(),
                    mime_type: Some("text/plain".into()),
                },
                ContentPart::text("\ndetails"),
            ],
            exit_code: None,
            is_error: false,
            output_truncated: false,
        };

        let capability_result = tool_result.clone().into_capability_result();
        assert_eq!(tool_result.text_output(), capability_result.text_output());

        let round_tripped = ToolResult::from_capability_result(capability_result);
        assert_eq!(tool_result.text_output(), round_tripped.text_output());
    }

    #[test]
    fn error_metadata_projection_stays_in_lockstep_across_capability_and_tool_results() {
        let capability_result = CapabilityResult {
            content: Vec::new(),
            exit_code: None,
            is_error: false,
            output_truncated: false,
            error: Some(CapabilityError::with_code("ACP_TIMEOUT", "timed out")),
        };

        let tool_result = ToolResult::from_capability_result(capability_result.clone());
        assert_eq!(capability_result.text_output(), tool_result.text_output());

        let back_to_capability = tool_result.into_capability_result();
        assert_eq!(
            capability_result.text_output(),
            back_to_capability.text_output()
        );
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
    fn tool_result_text_output_summarizes_resource_content_parts() {
        let result = ToolResult {
            content: vec![ContentPart::Resource {
                uri: "file://workspace/out.txt".into(),
                mime_type: Some("text/plain".into()),
            }],
            exit_code: None,
            is_error: false,
            output_truncated: false,
        };

        assert_eq!(
            result.text_output(),
            "[resource] file://workspace/out.txt (text/plain)"
        );
    }

    #[test]
    fn tool_result_text_output_separates_resource_and_following_text() {
        let result = ToolResult {
            content: vec![
                ContentPart::text("summary"),
                ContentPart::Resource {
                    uri: "file://workspace/out.txt".into(),
                    mime_type: Some("text/plain".into()),
                },
                ContentPart::text("details"),
            ],
            exit_code: None,
            is_error: false,
            output_truncated: false,
        };

        assert_eq!(
            result.text_output(),
            "summary\n[resource] file://workspace/out.txt (text/plain)\ndetails"
        );
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
            read_tool.resource_claims(&json!({"path":"./src/main.rs"})),
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
            write_tool.resource_claims(&json!({"path":"./src/main.rs"})),
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
        assert_eq!(agent_tool.kind(), ToolKind::Agent { is_acp: false });
    }

    #[test]
    fn normalized_workspace_resource_claim_collapses_dot_prefix_aliases() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        assert_eq!(
            normalize_workspace_resource_claim(&workspace, "src/main.rs").expect("normalized"),
            normalize_workspace_resource_claim(&workspace, "./src/main.rs")
                .expect("dot-prefixed path normalized")
        );
        assert!(normalize_workspace_resource_claim(&workspace, "../secret").is_err());
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

    #[test]
    fn local_capability_wrapper_preserves_normalized_claims_and_conflict_semantics() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let read_tool = read_file::ReadFileTool::new(workspace.clone());
        let write_tool = write_file::WriteFileTool::new(workspace);

        let read_capability = LocalToolCapability::from_tool(&read_tool);
        let write_capability = LocalToolCapability::from_tool(&write_tool);

        let normalized_read = read_capability.claims(&CapabilityInput::new(json!({
            "path": "src/main.rs"
        })));
        let dot_prefixed_read = read_capability.claims(&CapabilityInput::new(json!({
            "path": "./src/main.rs"
        })));
        let normalized_write = write_capability.claims(&CapabilityInput::new(json!({
            "path": "src/main.rs"
        })));

        assert_eq!(normalized_read, dot_prefixed_read);
        assert_eq!(
            ConflictPolicy::from_claims(&normalized_read, &normalized_write),
            ConflictPolicy::Conflicting
        );
    }

    #[test]
    fn local_capability_wrapper_preserves_local_tool_validation_behavior() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let read_tool = read_file::ReadFileTool::new(workspace.clone());
        let write_tool = write_file::WriteFileTool::new(workspace);

        let read_capability = LocalToolCapability::from_tool(&read_tool);
        let write_capability = LocalToolCapability::from_tool(&write_tool);

        let read_args = json!({ "path": "../secret" });
        let direct_read = read_tool.execute(read_args.clone());
        let wrapped_read = read_capability.execute(CapabilityInput::new(read_args));
        assert_eq!(wrapped_read.text_output(), direct_read.text_output());
        assert_eq!(wrapped_read.is_error, direct_read.is_error);

        let write_args = json!({ "path": "../secret", "content": "nope" });
        let direct_write = write_tool.execute(write_args.clone());
        let wrapped_write = write_capability.execute(CapabilityInput::new(write_args));
        assert_eq!(wrapped_write.text_output(), direct_write.text_output());
        assert_eq!(wrapped_write.is_error, direct_write.is_error);
    }

    #[test]
    fn tool_registry_routes_wrapper_path_by_tool_kind_policy() {
        struct RoutingProbeTool {
            name: &'static str,
            kind: ToolKind,
        }

        impl Tool for RoutingProbeTool {
            fn spec(&self) -> ToolSpec {
                ToolSpec {
                    name: self.name.into(),
                    description: "routing probe".into(),
                    parameters: json!({
                        "type": "object",
                        "properties": {},
                        "additionalProperties": false
                    }),
                }
            }

            fn kind(&self) -> ToolKind {
                self.kind
            }

            fn execute(&self, _args: Value) -> ToolResult {
                ToolResult {
                    content: Vec::new(),
                    exit_code: None,
                    is_error: true,
                    output_truncated: false,
                }
            }
        }

        let mut registry = ToolRegistry::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        registry.register(Box::new(RoutingProbeTool {
            name: "local-probe",
            kind: ToolKind::Local,
        }));
        registry.register(Box::new(RoutingProbeTool {
            name: "delegate-probe",
            kind: ToolKind::Agent { is_acp: false },
        }));
        registry.register(Box::new(RoutingProbeTool {
            name: "acp-probe",
            kind: ToolKind::Agent { is_acp: true },
        }));

        let local = registry
            .execute("local-probe", json!({}))
            .expect("local probe should execute");
        let delegated = registry
            .execute("delegate-probe", json!({}))
            .expect("delegate probe should execute");
        let acp = registry
            .execute("acp-probe", json!({}))
            .expect("acp probe should execute");

        let wrapped_projection = vec![ContentPart::Text {
            text: String::new(),
        }];
        assert_eq!(local.content, wrapped_projection);
        assert_eq!(
            delegated.content,
            vec![ContentPart::Text {
                text: String::new(),
            }]
        );
        assert!(acp.content.is_empty());
    }
}
