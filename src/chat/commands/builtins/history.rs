use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct HistoryCommand {
    spec: CommandSpec,
}

impl PaletteCommand for HistoryCommand {
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
                .push_system_hint("Conversation history browser coming soon");
            Ok(CommandResult {
                summary: "History view displayed".to_owned(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/history",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(HistoryCommand {
                spec: CommandSpec {
                    name: "/history".to_owned(),
                    description: "Browse conversation history".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
