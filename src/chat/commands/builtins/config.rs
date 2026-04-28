use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct ConfigCommand {
    spec: CommandSpec,
}

impl PaletteCommand for ConfigCommand {
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
                .push_system_hint("Configuration settings interface coming soon");
            Ok(CommandResult {
                summary: "Config view displayed".to_owned(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/config",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(ConfigCommand {
                spec: CommandSpec {
                    name: "/config".to_owned(),
                    description: "Open configuration settings".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
