use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct McpCommand {
    spec: CommandSpec,
}

impl PaletteCommand for McpCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app
                .push_system_hint("MCP server configuration interface coming soon");
            Ok(CommandResult {
                summary: "MCP config view displayed".to_owned(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/mcp",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(McpCommand {
                spec: CommandSpec {
                    name: "/mcp".to_owned(),
                    description: "Manage MCP server configuration".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
