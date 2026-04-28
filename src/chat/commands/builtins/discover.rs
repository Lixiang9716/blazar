use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

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

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/discover-agents",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(DiscoverAgentsCommand {
                spec: CommandSpec {
                    name: "/discover-agents".to_owned(),
                    description: "Refresh discovered ACP agents".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
