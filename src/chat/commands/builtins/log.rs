use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct LogCommand {
    spec: CommandSpec,
}

impl PaletteCommand for LogCommand {
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
                .push_system_hint("Application logs viewer coming soon");
            Ok(CommandResult {
                summary: "Log view displayed".to_owned(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/log",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(LogCommand {
                spec: CommandSpec {
                    name: "/log".to_owned(),
                    description: "Show application logs".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
