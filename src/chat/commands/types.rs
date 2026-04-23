use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type CommandExecFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CommandResult, CommandError>> + Send + 'a>>;

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub name: String,
    pub description: String,
    pub args_schema: Value,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    InvalidArgs(String),
    Unavailable(String),
    ExecutionFailed(String),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArgs(message) => write!(f, "invalid args: {message}"),
            Self::Unavailable(message) => write!(f, "unavailable: {message}"),
            Self::ExecutionFailed(message) => write!(f, "execution failed: {message}"),
        }
    }
}

impl std::error::Error for CommandError {}

pub trait PaletteCommand: Send + Sync {
    fn spec(&self) -> &CommandSpec;

    fn execute(&self, args: Value) -> CommandExecFuture<'_>;
}
