use std::sync::Arc;

use serde_json::json;

use crate::chat::commands::orchestrator::CommandContext;
use crate::chat::commands::plugin::{
    BuiltinCommandDescriptor, BuiltinCommandProfiles, CommandBuildContext,
};
use crate::chat::commands::types::{
    CommandError, CommandExecFuture, CommandResult, CommandSpec, PaletteCommand,
};

struct GitCommand {
    spec: CommandSpec,
}

impl PaletteCommand for GitCommand {
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

            let branch_output = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(workspace)
                .output()
                .map_err(|e| CommandError::ExecutionFailed(format!("git branch failed: {e}")))?;

            if !status_output.status.success() {
                return Err(CommandError::Unavailable("Not a git repository".to_owned()));
            }

            let status = String::from_utf8_lossy(&status_output.stdout);
            let branch = String::from_utf8_lossy(&branch_output.stdout)
                .trim()
                .to_owned();

            let status_lines: Vec<&str> = status.lines().collect();
            let summary = if status_lines.is_empty() {
                "working tree clean".to_owned()
            } else {
                format!("{} files changed", status_lines.len())
            };

            let body = format!("branch: {}", branch);
            let details = if status_lines.is_empty() {
                "working tree clean".to_owned()
            } else {
                status.trim().to_owned()
            };

            ctx.app.push_system_hint_with_details(body, details);

            Ok(CommandResult { summary })
        })
    }
}

inventory::submit! {
    BuiltinCommandDescriptor {
        name: "/git",
        profiles: BuiltinCommandProfiles::Interactive,
        build: |_ctx: &CommandBuildContext| {
            Arc::new(GitCommand {
                spec: CommandSpec {
                    name: "/git".to_owned(),
                    description: "Show git repository status".to_owned(),
                    args_schema: json!({ "type": "object" }),
                },
            })
        },
    }
}
