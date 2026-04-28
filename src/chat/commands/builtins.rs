use std::sync::Arc;

use serde_json::json;

use super::orchestrator::CommandContext;
use super::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext, CommandBuildProfile,
    build_commands_from_descriptors, collect_builtin_descriptors,
};
use super::registry::CommandRegistry;
use super::types::{CommandError, CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

fn spec(name: &str, description: &str) -> CommandSpec {
    CommandSpec {
        name: name.to_owned(),
        description: description.to_owned(),
        args_schema: json!({ "type": "object" }),
    }
}

// ---------------------------------------------------------------------------
// Command implementations (single source of truth)
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

struct PlanCommand {
    spec: CommandSpec,
}

impl PaletteCommand for PlanCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.set_composer_text("/plan ");
            Ok(CommandResult {
                summary: "Prepared /plan composer prompt".to_string(),
            })
        })
    }
}

struct DiscoverAgentsCommand {
    spec: CommandSpec,
}

impl PaletteCommand for DiscoverAgentsCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.execute_discover_agents_command();
            Ok(CommandResult {
                summary: "Queued ACP agent discovery".to_string(),
            })
        })
    }
}

struct ThemeCommand {
    spec: CommandSpec,
}

impl PaletteCommand for ThemeCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.open_theme_picker();
            Ok(CommandResult {
                summary: "Opened theme selector".to_string(),
            })
        })
    }
}

struct ModelCommand {
    spec: CommandSpec,
}

impl PaletteCommand for ModelCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.open_model_picker();
            Ok(CommandResult {
                summary: "Opened model selector".to_string(),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Inventory-backed built-in command registration (SINGLE SOURCE OF TRUTH)
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

register_forward_command!("/help", "Show available commands and shortcuts");
register_forward_command!("/clear", "Clear the conversation history");
register_forward_command!("/copy", "Copy the last response to the clipboard");
register_forward_command!("/init", "Generate a blazar-instructions.md file");
register_forward_command!("/skills", "List loaded skills and their status");
register_forward_command!("/mcp", "Manage MCP server configuration");
register_forward_command!("/history", "Browse conversation history");
register_forward_command!("/export", "Export conversation to file");
register_forward_command!("/compact", "Compact conversation context");
register_forward_command!("/config", "Open configuration settings");
register_forward_command!("/tools", "List available tools");
register_forward_command!("/agents", "List running background agents");
register_forward_command!("/context", "Show current context window usage");
register_forward_command!("/diff", "Show pending file changes");
register_forward_command!("/git", "Show git repository status");
register_forward_command!("/undo", "Undo last file change");
register_forward_command!("/terminal", "Open a shell terminal");
register_forward_command!("/debug", "Toggle debug overlay");
register_forward_command!("/log", "Show application logs");
register_forward_command!("/quit", "Exit Blazar");

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/model",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(ModelCommand {
                spec: spec("/model", "Switch the active model"),
            })
        },
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/theme",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(ThemeCommand {
                spec: spec("/theme", "Switch the color theme"),
            })
        },
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/plan",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(PlanCommand {
                spec: spec("/plan", "Generate a plan with an auto-titled summary"),
            })
        },
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/discover-agents",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(DiscoverAgentsCommand {
                spec: spec("/discover-agents", "Refresh discovered ACP agents"),
            })
        },
    }
}

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
