use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct ContextCommand {
    spec: CommandSpec,
}

impl PaletteCommand for ContextCommand {
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
                .push_system_hint("Context window usage tracking coming soon");
            Ok(CommandResult {
                summary: "Context information displayed".to_owned(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/context",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(ContextCommand {
                spec: CommandSpec {
                    name: "/context".to_owned(),
                    description: "Show current context window usage".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
