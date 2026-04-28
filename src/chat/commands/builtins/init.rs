use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{
    CommandError, CommandExecFuture, CommandResult, CommandSpec, PaletteCommand,
};

const INSTRUCTIONS_TEMPLATE: &str = r#"# Blazar Instructions

This file guides Blazar's behavior in this workspace.

## Context

Brief description of the project and its purpose.

## Preferences

- Coding style and conventions
- Testing requirements
- Documentation standards
- File organization patterns

## Constraints

- Technologies to use or avoid
- Performance requirements
- Security considerations

## Commands

List any project-specific commands or workflows.
"#;

struct InitCommand {
    spec: CommandSpec,
}

impl PaletteCommand for InitCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            let workspace = ctx.app.workspace_root();
            let target = workspace.join("blazar-instructions.md");

            if target.exists() {
                ctx.app
                    .push_system_hint("blazar-instructions.md already exists");
                return Ok(CommandResult {
                    summary: "Instructions file already exists".to_owned(),
                });
            }

            std::fs::write(&target, INSTRUCTIONS_TEMPLATE).map_err(|e| {
                CommandError::ExecutionFailed(format!("Failed to write instructions file: {e}"))
            })?;

            ctx.app.push_system_hint(format!(
                "Created blazar-instructions.md in {}",
                workspace.display()
            ));

            Ok(CommandResult {
                summary: "Created blazar-instructions.md".to_owned(),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/init",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(InitCommand {
                spec: CommandSpec {
                    name: "/init".to_owned(),
                    description: "Generate a blazar-instructions.md file".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
