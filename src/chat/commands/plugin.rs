use std::sync::Arc;

use super::types::PaletteCommand;

/// Selects which assembly context a built-in command is being constructed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandBuildProfile {
    Interactive,
}

/// Declares which profiles a built-in descriptor participates in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinCommandProfiles {
    Interactive,
}

impl BuiltinCommandProfiles {
    /// Returns `true` when the descriptor should be included for `profile`.
    pub fn includes(self, profile: CommandBuildProfile) -> bool {
        match (self, profile) {
            (Self::Interactive, CommandBuildProfile::Interactive) => true,
        }
    }
}

/// Runtime dependencies passed to built-in command constructors.
pub struct CommandBuildContext;

/// A compile-time descriptor for a built-in command registered via `inventory`.
///
/// Each built-in command module submits one of these. At runtime,
/// the registry collects them, filters by profile, and builds
/// concrete [`PaletteCommand`] instances through the `build` function pointer.
pub struct BuiltinCommandDescriptor {
    pub name: &'static str,
    pub profiles: BuiltinCommandProfiles,
    pub build: fn(&CommandBuildContext) -> Arc<dyn PaletteCommand>,
}

inventory::collect!(BuiltinCommandDescriptor);

/// Collects built-in command descriptors matching `profile` and returns them.
///
/// Descriptors are sorted by name for deterministic registration order.
pub fn collect_builtin_descriptors(
    profile: CommandBuildProfile,
) -> Vec<&'static BuiltinCommandDescriptor> {
    let mut descriptors: Vec<&BuiltinCommandDescriptor> =
        inventory::iter::<BuiltinCommandDescriptor>
            .into_iter()
            .filter(|d| d.profiles.includes(profile))
            .collect();
    descriptors.sort_by_key(|d| d.name);
    descriptors
}

/// Internal helper that builds commands from descriptors and validates them.
///
/// Steps: reject duplicates → build → validate name match.
pub fn build_commands_from_descriptors(
    descriptors: &[&BuiltinCommandDescriptor],
    ctx: &CommandBuildContext,
) -> Result<Vec<Arc<dyn PaletteCommand>>, String> {
    // Reject duplicate descriptor names.
    for window in descriptors.windows(2) {
        if window[0].name == window[1].name {
            return Err(format!(
                "duplicate built-in command descriptor name: {}",
                window[0].name
            ));
        }
    }

    let mut commands: Vec<Arc<dyn PaletteCommand>> = Vec::with_capacity(descriptors.len());
    for descriptor in descriptors {
        let command = (descriptor.build)(ctx);
        let spec = command.spec();
        if spec.name != descriptor.name {
            return Err(format!(
                "built-in command descriptor name mismatch: descriptor declares '{}' but command advertises '{}'",
                descriptor.name, spec.name
            ));
        }
        commands.push(command);
    }
    Ok(commands)
}
