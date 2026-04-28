use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct ModelCommand {
    spec: CommandSpec,
}

impl PaletteCommand for ModelCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.open_model_picker();
            Ok(CommandResult {
                summary: "Opened model selector".to_string(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/model",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(ModelCommand {
                spec: CommandSpec {
                    name: "/model".to_owned(),
                    description: "Switch the active model".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
