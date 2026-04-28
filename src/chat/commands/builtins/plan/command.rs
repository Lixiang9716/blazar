use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

use super::store::PlanStore;

struct PlanCommand {
    spec: CommandSpec,
}

impl PaletteCommand for PlanCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            let session = PlanStore::new().create_session();
            ctx.app.set_composer_text(session.prefill_text());
            Ok(CommandResult {
                summary: "Prepared /plan composer prompt".to_string(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/plan",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(PlanCommand {
                spec: CommandSpec {
                    name: "/plan".to_owned(),
                    description: "Generate a plan with an auto-titled summary".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
