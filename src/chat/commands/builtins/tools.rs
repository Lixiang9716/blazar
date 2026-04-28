use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct ToolsCommand {
    spec: CommandSpec,
}

impl PaletteCommand for ToolsCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.push_system_hint("Tool listing coming soon");
            Ok(CommandResult {
                summary: "Tools information displayed".to_owned(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/tools",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(ToolsCommand {
                spec: CommandSpec {
                    name: "/tools".to_owned(),
                    description: "List available tools".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
