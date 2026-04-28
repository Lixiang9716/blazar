use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct ClearCommand {
    spec: CommandSpec,
}

impl PaletteCommand for ClearCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.clear_conversation();
            Ok(CommandResult {
                summary: "Conversation cleared".to_string(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/clear",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(ClearCommand {
                spec: CommandSpec {
                    name: "/clear".to_owned(),
                    description: "Clear the conversation history".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
