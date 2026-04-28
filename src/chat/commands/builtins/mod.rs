use std::sync::Arc;

use serde_json::json;

use super::orchestrator::CommandContext;
use super::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext, CommandBuildProfile,
    build_commands_from_descriptors, collect_builtin_descriptors,
};
use super::registry::CommandRegistry;
use super::types::{CommandError, CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

pub mod agents;
pub mod clear;
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

fn spec(name: &str, description: &str) -> CommandSpec {
    CommandSpec {
        name: name.to_owned(),
        description: description.to_owned(),
        args_schema: json!({ "type": "object" }),
    }
}

// ---------------------------------------------------------------------------
// Command implementations (commands not yet split into modules)
// ---------------------------------------------------------------------------

struct ForwardCommand {
    spec: CommandSpec,
}

impl PaletteCommand for ForwardCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        let command = self.spec.name.clone();
        Box::pin(async move {
            ctx.app.send_message_without_command_dispatch(&command);
            Ok(CommandResult {
                summary: format!("Queued {command}"),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Inventory-backed built-in command registration (commands not yet split)
// ---------------------------------------------------------------------------

macro_rules! register_forward_command {
    ($name:literal, $description:literal) => {
        inventory::submit! {
            BuiltinCommandDescriptor {
                name: $name,
                profiles: BuiltinCommandProfiles::Interactive,
                build: |_ctx: &CommandBuildContext| {
                    Arc::new(ForwardCommand {
                        spec: spec($name, $description),
                    })
                },
            }
        }
    };
}

// Task 6 command (workspace side effect - to be implemented)
register_forward_command!("/compact", "Compact conversation context");

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
