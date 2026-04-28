use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{
    CommandError, CommandExecFuture, CommandResult, CommandSpec, PaletteCommand,
};

const MAX_DIFF_LINES: usize = 100;

struct DiffCommand {
    spec: CommandSpec,
}

impl PaletteCommand for DiffCommand {
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

            let status_output = std::process::Command::new("git")
                .args(["status", "--short"])
                .current_dir(workspace)
                .output()
                .map_err(|e| CommandError::ExecutionFailed(format!("git status failed: {e}")))?;

            if !status_output.status.success() {
                return Err(CommandError::Unavailable("Not a git repository".to_owned()));
            }

            let status = String::from_utf8_lossy(&status_output.stdout);
            let changed_files = status.lines().count();

            if changed_files == 0 {
                ctx.app.push_system_hint("No changes to show");
                return Ok(CommandResult {
                    summary: "No changes".to_owned(),
                });
            }

            let diff_output = std::process::Command::new("git")
                .args(["diff", "--stat"])
                .current_dir(workspace)
                .output()
                .map_err(|e| CommandError::ExecutionFailed(format!("git diff failed: {e}")))?;

            let diff = String::from_utf8_lossy(&diff_output.stdout);
            let diff_lines: Vec<&str> = diff.lines().take(MAX_DIFF_LINES).collect();
            let truncated = diff.lines().count() > MAX_DIFF_LINES;

            let body = if truncated {
                format!(
                    "{}\n\n(truncated to {} lines)",
                    diff_lines.join("\n"),
                    MAX_DIFF_LINES
                )
            } else {
                diff_lines.join("\n")
            };

            ctx.app.push_system_hint(body);

            Ok(CommandResult {
                summary: format!("{} files changed", changed_files),
            })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/diff",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(DiffCommand {
                spec: CommandSpec {
                    name: "/diff".to_owned(),
                    description: "Show pending file changes".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
