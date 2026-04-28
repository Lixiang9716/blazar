use std::sync::Arc;

use serde_json::Value;

use super::{CommandError, CommandRegistry, CommandResult, PaletteCommand};

pub struct CommandContext<'a> {
    pub app: &'a mut crate::chat::app::ChatApp,
}

pub async fn execute_palette_command(
    registry: &CommandRegistry,
    app: &mut crate::chat::app::ChatApp,
    name: &str,
    args: Value,
) -> Result<CommandResult, CommandError> {
    let command = registry
        .find(name)
        .cloned()
        .ok_or_else(|| CommandError::Unavailable(format!("unknown command: {name}")))?;

    execute_palette_command_from_command(command, app, args).await
}

pub(crate) async fn execute_palette_command_from_command(
    command: Arc<dyn PaletteCommand>,
    app: &mut crate::chat::app::ChatApp,
    args: Value,
) -> Result<CommandResult, CommandError> {
    let mut ctx = CommandContext { app };
    command.execute(&mut ctx, args).await
}

pub async fn execute_palette_command_for_test(
    app: &mut crate::chat::app::ChatApp,
    name: &str,
    args: Value,
) -> Result<CommandResult, CommandError> {
    let registry = CommandRegistry::with_builtins()
        .map_err(|e| CommandError::ExecutionFailed(format!("failed to bootstrap registry: {e}")))?;
    execute_palette_command(&registry, app, name, args).await
}
