use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct ThemeCommand {
    spec: CommandSpec,
}

impl PaletteCommand for ThemeCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.open_theme_picker();
            Ok(CommandResult {
                summary: "Opened theme selector".to_string(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/theme",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(ThemeCommand {
                spec: CommandSpec {
                    name: "/theme".to_owned(),
                    description: "Switch the color theme".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
