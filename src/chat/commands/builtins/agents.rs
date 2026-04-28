use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct AgentsCommand {
    spec: CommandSpec,
}

impl PaletteCommand for AgentsCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.push_system_hint("Agent status display coming soon");
            Ok(CommandResult {
                summary: "Agent information displayed".to_owned(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/agents",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(AgentsCommand {
                spec: CommandSpec {
                    name: "/agents".to_owned(),
                    description: "List running background agents".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
