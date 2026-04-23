use std::sync::Arc;

use serde_json::json;

use super::orchestrator::CommandContext;
use super::registry::CommandRegistry;
use super::types::{CommandError, CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

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

fn spec(name: &str, description: &str) -> CommandSpec {
    CommandSpec {
        name: name.to_owned(),
        description: description.to_owned(),
        args_schema: json!({ "type": "object" }),
    }
}

fn register_forward(
    registry: &mut CommandRegistry,
    name: &str,
    description: &str,
) -> Result<(), CommandError> {
    registry.register(Arc::new(ForwardCommand {
        spec: spec(name, description),
    }))
}

pub fn register_builtin_commands(registry: &mut CommandRegistry) -> Result<(), CommandError> {
    register_forward(registry, "/help", "Show available commands and shortcuts")?;
    register_forward(registry, "/clear", "Clear the conversation history")?;
    register_forward(registry, "/copy", "Copy the last response to the clipboard")?;
    register_forward(registry, "/init", "Generate a blazar-instructions.md file")?;
    register_forward(registry, "/skills", "List loaded skills and their status")?;

    registry.register(Arc::new(ModelCommand {
        spec: spec("/model", "Switch the active model"),
    }))?;

    register_forward(registry, "/mcp", "Manage MCP server configuration")?;

    registry.register(Arc::new(ThemeCommand {
        spec: spec("/theme", "Switch the color theme"),
    }))?;

    register_forward(registry, "/history", "Browse conversation history")?;

    registry.register(Arc::new(PlanCommand {
        spec: spec("/plan", "Generate a plan with an auto-titled summary"),
    }))?;

    register_forward(registry, "/export", "Export conversation to file")?;
    register_forward(registry, "/compact", "Compact conversation context")?;
    register_forward(registry, "/config", "Open configuration settings")?;
    register_forward(registry, "/tools", "List available tools")?;
    register_forward(registry, "/agents", "List running background agents")?;

    registry.register(Arc::new(DiscoverAgentsCommand {
        spec: spec("/discover-agents", "Refresh discovered ACP agents"),
    }))?;

    register_forward(registry, "/context", "Show current context window usage")?;
    register_forward(registry, "/diff", "Show pending file changes")?;
    register_forward(registry, "/git", "Show git repository status")?;
    register_forward(registry, "/undo", "Undo last file change")?;
    register_forward(registry, "/terminal", "Open a shell terminal")?;
    register_forward(registry, "/debug", "Toggle debug overlay")?;
    register_forward(registry, "/log", "Show application logs")?;
    register_forward(registry, "/quit", "Exit Blazar")?;

    Ok(())
}
