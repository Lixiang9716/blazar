pub mod builtins;
pub mod registry;
pub mod types;

pub use registry::CommandRegistry;
pub use types::{CommandError, CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};
