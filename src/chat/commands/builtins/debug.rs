use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct DebugCommand {
    spec: CommandSpec,
}

impl PaletteCommand for DebugCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.toggle_debug_overlay();
            let state = if ctx.app.show_details() {
                "enabled"
            } else {
                "disabled"
            };
            ctx.app.push_system_hint(format!("Debug overlay {state}"));
            Ok(CommandResult {
                summary: format!("Debug overlay {state}"),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/debug",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(DebugCommand {
                spec: CommandSpec {
                    name: "/debug".to_owned(),
                    description: "Toggle debug overlay".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
