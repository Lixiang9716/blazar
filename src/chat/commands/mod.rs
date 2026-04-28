pub mod builtins;
pub mod matcher;
pub mod orchestrator;
pub mod plugin;
pub mod registry;
pub mod types;

pub use registry::CommandRegistry;
pub use types::{CommandError, CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};
