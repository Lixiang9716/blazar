use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct CompactCommand {
    spec: CommandSpec,
}

impl PaletteCommand for CompactCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.execute_compact_command();
            Ok(CommandResult {
                summary: "Compaction started".to_string(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/compact",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(CompactCommand {
                spec: CommandSpec {
                    name: "/compact".to_owned(),
                    description: "Compact conversation context with local LLM summary".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
