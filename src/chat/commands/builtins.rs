use std::sync::Arc;

use serde_json::json;

use super::registry::CommandRegistry;
use super::types::{CommandError, CommandExecFuture, CommandResult, CommandSpec, PaletteCommand};

struct StubCommand {
    spec: CommandSpec,
}

impl PaletteCommand for StubCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    fn execute(&self, _args: serde_json::Value) -> CommandExecFuture<'_> {
        Box::pin(async {
            Ok(CommandResult {
                summary: "ok".to_string(),
            })
        })
    }
}

pub fn register_builtin_commands(registry: &mut CommandRegistry) -> Result<(), CommandError> {
    for (name, description) in [
        ("/plan", "Generate a plan with an auto-titled summary"),
        ("/discover-agents", "Refresh discovered ACP agents"),
    ] {
        registry.register(Arc::new(StubCommand {
            spec: CommandSpec {
                name: name.to_string(),
                description: description.to_string(),
                args_schema: json!({ "type": "object" }),
            },
        }))?;
    }

    Ok(())
}
