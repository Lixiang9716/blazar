use std::sync::Arc;

use serde_json::json;

use super::orchestrator::CommandContext;
use super::registry::CommandRegistry;
use super::types::{CommandError, CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

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
            ctx.app.send_message("/discover-agents");
            Ok(CommandResult {
                summary: "Queued ACP agent discovery".to_string(),
            })
        })
    }
}

pub fn register_builtin_commands(registry: &mut CommandRegistry) -> Result<(), CommandError> {
    registry.register(Arc::new(PlanCommand {
        spec: CommandSpec {
            name: "/plan".to_string(),
            description: "Generate a plan with an auto-titled summary".to_string(),
            args_schema: json!({ "type": "object" }),
        },
    }))?;

    registry.register(Arc::new(DiscoverAgentsCommand {
        spec: CommandSpec {
            name: "/discover-agents".to_string(),
            description: "Refresh discovered ACP agents".to_string(),
            args_schema: json!({ "type": "object" }),
        },
    }))?;

    Ok(())
}
