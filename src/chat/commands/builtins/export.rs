use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{
    CommandError, CommandExecFuture, CommandResult, CommandSpec, PaletteCommand,
};

struct ExportCommand {
    spec: CommandSpec,
}

impl PaletteCommand for ExportCommand {
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
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let filename = format!("blazar-export-{}.json", timestamp);
            let target = workspace.join(&filename);

            let conversation = ctx.app.export_conversation_json();
            let json_str = serde_json::to_string_pretty(&conversation).map_err(|e| {
                CommandError::ExecutionFailed(format!("Failed to serialize conversation: {e}"))
            })?;

            std::fs::write(&target, json_str).map_err(|e| {
                CommandError::ExecutionFailed(format!("Failed to write export file: {e}"))
            })?;

            let relative_path = target
                .strip_prefix(workspace)
                .unwrap_or(&target)
                .display()
                .to_string();

            ctx.app
                .push_system_hint(format!("Exported conversation to {}", relative_path));

            Ok(CommandResult {
                summary: format!("Exported to {}", relative_path),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/export",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(ExportCommand {
                spec: CommandSpec {
                    name: "/export".to_owned(),
                    description: "Export conversation to file".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
