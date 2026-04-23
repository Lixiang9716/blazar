# Command Palette Declarative Registry Design

## Summary

Refactor `command_palette` from a hard-coded item list into a declarative command registry.
Each command is registered as a trait implementation with:

1. command name
2. command description
3. accepted argument schema
4. async callback execution

The palette only renders discoverable commands and selection state; command execution is routed through a dedicated orchestrator.

## Problem

Current `src/chat/picker.rs::command_palette()` hard-codes `PickerItem` entries, while execution logic is handled elsewhere via imperative branching. This causes:

- drift risk between displayed commands and executable commands
- weak extensibility for command metadata and argument validation
- poor composability for async command execution

## Goals

1. Make all built-in `/xxx` commands declarative and registry-backed.
2. Keep command metadata (name/description/args) in one source of truth.
3. Support async command callback execution with structured arguments.
4. Provide deterministic command matching and ranking while typing `/...`.

## Non-Goals

1. No ACP/protocol redesign in this change.
2. No new visual surfaces beyond command palette behavior needed for command discovery/execution.
3. No generic plugin marketplace design in this phase.

## Design Decisions (validated)

1. Use trait + registry as the command declaration model.
2. Command callbacks support async execution.
3. All existing built-in commands are migrated to registry registration (no mixed static list).
4. Matching triggers only when input starts with `/`.
5. Matching mode is layered ranking: `exact > prefix > contains > fuzzy`.

## Proposed Architecture

### 1) Command Trait

Each command implements a shared trait (name illustrative):

- `spec() -> CommandSpec` with `name`, `description`, `args_schema`
- `execute(ctx, args) -> Future<Result<CommandResult, CommandError>>`

`args_schema` is used for argument collection and validation before callback execution.

### 2) Command Registry

`CommandRegistry` owns all command declarations and lookup:

- `register(command)`
- `list() -> &[CommandSpec]` (for palette rendering)
- `find(name) -> Option<&dyn Command>`

The registry is the single source for both display and dispatch.

### 3) Command Orchestrator

`CommandOrchestrator` is the execution boundary:

1. resolve command by selected name
2. collect/validate args from `args_schema`
3. invoke async callback
4. normalize success/failure output into timeline/status updates

This keeps UI/picker state separate from command business logic.

### 4) Command Matcher

`CommandMatcher` computes ranked matches for `/...` input:

- normalize query (`trim + lowercase`)
- evaluate candidates by tier:
  1. exact
  2. prefix
  3. contains (name or description)
  4. fuzzy subsequence
- stable order by `(tier asc, score desc, name asc)`

## Interaction/Data Flow

1. User types `/` in composer.
2. Palette opens in `Commands` context and requests `registry.list()`.
3. As user types `/xxx`, matcher returns ranked candidates.
4. User confirms selection.
5. Orchestrator resolves command, prepares args, executes async callback, and writes result to timeline/status.

Behavior notes:

- Query `/` (empty suffix) shows all commands in default order.
- No matches shows an explicit empty-state item.
- Matching is case-insensitive.

## Error Handling

Unify errors behind `CommandError`:

- `InvalidArgs`
- `Unavailable`
- `ExecutionFailed`

Orchestrator maps errors to user-facing messages and keeps the application state consistent.

## Migration Plan (Scope)

1. Replace static `command_palette()` command list with registry-derived items.
2. Move all built-in `/xxx` command definitions to trait implementations.
3. Remove duplicate imperative command declarations that become obsolete.
4. Keep existing command behavior contract unchanged unless explicitly re-specified.

## Testing Strategy

1. Registry tests
   - all built-ins are registered
   - command names are unique
2. Matcher tests
   - exact/prefix/contains/fuzzy ranking order
   - case-insensitive behavior
   - `/` empty suffix and no-match states
3. Palette tests
   - list comes from registry, not static literals
4. Orchestrator tests
   - async callback success path
   - invalid args failure path
   - execution failure path
5. Regression tests
   - existing key built-ins still execute with expected outcomes

## Acceptance Criteria

1. `command_palette` displays command name + description from registry metadata.
2. All built-in commands are declarative trait registrations.
3. Selected command executes via async callback with validated args.
4. `/...` matching follows the agreed layered ranking behavior.
5. Existing command behaviors remain functionally equivalent after migration.
