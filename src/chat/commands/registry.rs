use std::collections::HashMap;
use std::sync::Arc;

use super::plugin::{
    CommandBuildContext, CommandBuildProfile, build_commands_from_descriptors,
    collect_builtin_descriptors,
};
use super::types::{CommandError, CommandSpec, PaletteCommand};

#[derive(Default)]
pub struct CommandRegistry {
    ordered: Vec<Arc<dyn PaletteCommand>>,
    by_name: HashMap<String, Arc<dyn PaletteCommand>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a registry and populates it with built-in commands from inventory.
    ///
    /// This is the preferred bootstrap path for production and test code that
    /// needs the full set of interactive commands.
    pub fn with_builtins() -> Result<Self, CommandError> {
        let mut registry = Self::new();
        let ctx = CommandBuildContext;
        let descriptors = collect_builtin_descriptors(CommandBuildProfile::Interactive);
        let commands = build_commands_from_descriptors(&descriptors, &ctx)
            .map_err(CommandError::ExecutionFailed)?;

        for command in commands {
            registry.register(command)?;
        }

        Ok(registry)
    }

    pub fn register(&mut self, command: Arc<dyn PaletteCommand>) -> Result<(), CommandError> {
        let name = command.spec().name.clone();
        if self.by_name.contains_key(&name) {
            return Err(CommandError::ExecutionFailed(format!(
                "duplicate command: {name}"
            )));
        }

        self.by_name.insert(name, Arc::clone(&command));
        self.ordered.push(command);
        Ok(())
    }

    pub fn list(&self) -> Vec<&CommandSpec> {
        self.ordered.iter().map(|command| command.spec()).collect()
    }

    pub fn find(&self, name: &str) -> Option<&Arc<dyn PaletteCommand>> {
        self.by_name.get(name)
    }
}
