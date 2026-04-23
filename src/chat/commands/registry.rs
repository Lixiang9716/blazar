use std::collections::HashMap;
use std::sync::Arc;

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
