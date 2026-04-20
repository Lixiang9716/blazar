use serde_json::Value;
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
