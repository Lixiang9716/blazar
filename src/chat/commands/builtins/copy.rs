use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{
    CommandError, CommandExecFuture, CommandResult, CommandSpec, PaletteCommand,
};

struct CopyCommand {
    spec: CommandSpec,
}

impl PaletteCommand for CopyCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async move {
            let Some(body) = ctx.app.last_assistant_message() else {
                return Err(CommandError::Unavailable(
                    "No assistant message to copy".to_owned(),
                ));
            };

            match arboard::Clipboard::new() {
                Ok(mut clipboard) => match clipboard.set_text(&body) {
                    Ok(()) => Ok(CommandResult {
                        summary: format!("Copied {} characters to clipboard", body.len()),
                    }),
                    Err(e) => Err(CommandError::ExecutionFailed(format!(
                        "Failed to set clipboard: {e}"
                    ))),
                },
                Err(e) => Err(CommandError::Unavailable(format!(
                    "Clipboard unavailable: {e}"
                ))),
            }
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/copy",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(CopyCommand {
                spec: CommandSpec {
                    name: "/copy".to_owned(),
                    description: "Copy the last response to the clipboard".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
