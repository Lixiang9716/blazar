pub mod bash;
pub mod list_dir;
pub mod read_file;
pub mod write_file;

use serde_json::Value;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    pub output: String,
    pub exit_code: Option<i32>,
    pub is_error: bool,
    pub output_truncated: bool,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            exit_code: None,
            is_error: false,
            output_truncated: false,
        }
    }

    pub fn failure(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            exit_code: None,
            is_error: true,
            output_truncated: false,
        }
    }
}

pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn execute(&self, args: Value) -> ToolResult;
}

pub struct ToolRegistry {
    workspace_root: PathBuf,
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            tools: Vec::new(),
        }
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|tool| tool.spec().name == name)
            .map(|tool| tool.as_ref())
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.iter().map(|tool| tool.spec()).collect()
    }

    pub fn execute(&self, name: &str, args: Value) -> Result<ToolResult, String> {
        match self.get(name) {
            Some(tool) => Ok(tool.execute(args)),
            None => Err(format!("unknown tool: {name}")),
        }
    }
}

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
