use super::plugin::{
    CommandBuildContext, CommandBuildProfile, build_commands_from_descriptors,
    collect_builtin_descriptors,
};
use super::registry::CommandRegistry;
use super::types::CommandError;

pub mod agents;
pub mod clear;
pub mod compact;
pub mod config;
pub mod context;
pub mod copy;
pub mod debug;
pub mod diff;
pub mod discover;
pub mod export;
pub mod git;
pub mod help;
pub mod history;
pub mod init;
pub mod log;
pub mod mcp;
pub mod model;
pub mod plan;
pub mod quit;
pub mod skills;
pub mod terminal;
pub mod theme;
pub mod tools;
pub mod undo;

/// Compatibility shim for manual registration.
///
/// This function now consumes the same inventory-registered descriptors used by
/// `CommandRegistry::with_builtins()`, ensuring a single source of truth.
pub fn register_builtin_commands(registry: &mut CommandRegistry) -> Result<(), CommandError> {
    let ctx = CommandBuildContext;
    let descriptors = collect_builtin_descriptors(CommandBuildProfile::Interactive);
    let commands = build_commands_from_descriptors(&descriptors, &ctx)
        .map_err(CommandError::ExecutionFailed)?;

    for command in commands {
        registry.register(command)?;
    }

    Ok(())
}
