use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct UndoCommand {
    spec: CommandSpec,
}

impl PaletteCommand for UndoCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            // For Task 5, provide safe informational behavior.
            // Full undo tracking requires agent state integration (future work).
            ctx.app.push_system_hint(
                "File undo tracking not yet available. Use 'git checkout -- <file>' manually.",
            );

            Ok(CommandResult {
                summary: "Undo hint displayed".to_owned(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/undo",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(UndoCommand {
                spec: CommandSpec {
                    name: "/undo".to_owned(),
                    description: "Undo last file change".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
