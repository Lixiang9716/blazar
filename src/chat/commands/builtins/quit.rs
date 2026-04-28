use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct QuitCommand {
    spec: CommandSpec,
}

impl PaletteCommand for QuitCommand {
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

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/quit",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(QuitCommand {
                spec: CommandSpec {
                    name: "/quit".to_owned(),
                    description: "Exit Blazar".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
