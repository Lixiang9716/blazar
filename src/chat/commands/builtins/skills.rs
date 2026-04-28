use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct SkillsCommand {
    spec: CommandSpec,
}

impl PaletteCommand for SkillsCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.push_system_hint("Skills listing coming soon");
            Ok(CommandResult {
                summary: "Skills information displayed".to_owned(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/skills",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(SkillsCommand {
                spec: CommandSpec {
                    name: "/skills".to_owned(),
                    description: "List loaded skills and their status".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
