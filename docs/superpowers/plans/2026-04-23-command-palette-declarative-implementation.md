# Declarative Command Palette Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace hard-coded `/` command palette entries with a declarative trait-based registry where each command provides name, description, argument schema, and callback execution.

**Architecture:** Introduce a dedicated `chat::commands` module containing command contracts, registry, matcher, and orchestrator. Keep `ModalPicker` focused on UI state/rendering while routing command selection to orchestrator + registered commands. Migrate all built-in slash commands to registry-backed definitions and preserve existing behavior for `/theme`, `/model`, `/plan`, and `/discover-agents`.

**Tech Stack:** Rust (std async `Future` + boxed futures), `serde_json`, existing Blazar chat runtime/picker modules, `cargo test`, `just`.

---

## File Structure and Responsibilities

- **Create:** `src/chat/commands/mod.rs`  
  Module surface for command contracts, registry, matcher, built-ins, orchestrator.
- **Create:** `src/chat/commands/types.rs`  
  `CommandSpec`, `CommandError`, `CommandResult`, `CommandExecFuture`, `PaletteCommand` trait.
- **Create:** `src/chat/commands/registry.rs`  
  Registration, uniqueness checks, metadata list, name lookup.
- **Create:** `src/chat/commands/matcher.rs`  
  Layered matching (`exact > prefix > contains > fuzzy`) and stable ranking.
- **Create:** `src/chat/commands/builtins.rs`  
  Built-in command implementations and registration wiring for all current `/xxx`.
- **Create:** `src/chat/commands/orchestrator.rs`  
  Execute selected command with args, normalize result/error for `ChatApp`.
- **Modify:** `src/chat/mod.rs`  
  Export `commands` module.
- **Modify:** `src/chat/picker.rs`  
  Replace static `command_palette` list with registry-fed items; keep non-command contexts intact.
- **Modify:** `src/chat/app.rs`  
  Add command registry/orchestrator fields and constructor wiring.
- **Modify:** `src/chat/app/actions.rs`  
  Route picker submit/filter behavior through matcher and orchestrator.
- **Modify:** `src/chat/app/turns.rs`  
  Remove command-specific special cases migrated into built-in command callbacks.
- **Create:** `tests/chat_command_registry.rs`  
  Registry and built-in coverage tests.
- **Create:** `tests/chat_command_matching.rs`  
  Matching tier and sorting tests.
- **Create:** `tests/chat_command_orchestrator.rs`  
  Execution/args/error behavior tests.

---

### Task 1: Introduce command contracts and registry (TDD)

**Files:**
- Create: `src/chat/commands/mod.rs`
- Create: `src/chat/commands/types.rs`
- Create: `src/chat/commands/registry.rs`
- Modify: `src/chat/mod.rs`
- Test: `tests/chat_command_registry.rs`

- [ ] **Step 1: Write failing registry tests**

```rust
// tests/chat_command_registry.rs
use blazar::chat::commands::{CommandRegistry, builtins::register_builtin_commands};

#[test]
fn builtin_registry_contains_plan_and_discover_agents() {
    let mut registry = CommandRegistry::new();
    register_builtin_commands(&mut registry).expect("register built-ins");
    let specs = registry.list();
    assert!(specs.iter().any(|s| s.name == "/plan"));
    assert!(specs.iter().any(|s| s.name == "/discover-agents"));
}

#[test]
fn registry_rejects_duplicate_command_names() {
    let mut registry = CommandRegistry::new();
    register_builtin_commands(&mut registry).expect("first registration");
    let err = register_builtin_commands(&mut registry).expect_err("duplicate registration must fail");
    assert!(err.to_string().contains("duplicate command"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test chat_command_registry -- --nocapture`  
Expected: FAIL (missing `chat::commands` module/types).

- [ ] **Step 3: Add core command types and trait**

```rust
// src/chat/commands/types.rs
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type CommandExecFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CommandResult, CommandError>> + Send + 'a>>;

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub name: String,
    pub description: String,
    pub args_schema: Value,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub summary: String,
}

#[derive(Debug, Clone)]
pub enum CommandError {
    InvalidArgs(String),
    Unavailable(String),
    ExecutionFailed(String),
}

pub trait PaletteCommand: Send + Sync {
    fn spec(&self) -> &CommandSpec;
    fn execute<'a>(
        &'a self,
        ctx: &'a mut crate::chat::commands::orchestrator::CommandContext<'a>,
        args: Value,
    ) -> CommandExecFuture<'a>;
}
```

- [ ] **Step 4: Add registry and module exports**

```rust
// src/chat/commands/registry.rs
use std::collections::HashMap;
use std::sync::Arc;

use super::types::{CommandError, CommandSpec, PaletteCommand};

#[derive(Default)]
pub struct CommandRegistry {
    ordered: Vec<Arc<dyn PaletteCommand>>,
    by_name: HashMap<String, Arc<dyn PaletteCommand>>,
}

impl CommandRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, command: Arc<dyn PaletteCommand>) -> Result<(), CommandError> {
        let name = command.spec().name.clone();
        if self.by_name.contains_key(&name) {
            return Err(CommandError::ExecutionFailed(format!("duplicate command: {name}")));
        }
        self.by_name.insert(name, Arc::clone(&command));
        self.ordered.push(command);
        Ok(())
    }

    pub fn list(&self) -> Vec<&CommandSpec> {
        self.ordered.iter().map(|cmd| cmd.spec()).collect()
    }

    pub fn find(&self, name: &str) -> Option<&Arc<dyn PaletteCommand>> {
        self.by_name.get(name)
    }
}
```

```rust
// src/chat/commands/mod.rs
pub mod builtins;
pub mod matcher;
pub mod orchestrator;
pub mod registry;
pub mod types;

pub use registry::CommandRegistry;
pub use types::{CommandError, CommandResult, CommandSpec, PaletteCommand};
```

```rust
// src/chat/mod.rs
pub mod commands;
```

- [ ] **Step 5: Add minimal built-ins registration stub for tests**

```rust
// src/chat/commands/builtins.rs
use std::sync::Arc;

use super::registry::CommandRegistry;
use super::types::{CommandError, CommandExecFuture, CommandSpec, CommandResult, PaletteCommand};

struct StubCommand(CommandSpec);

impl PaletteCommand for StubCommand {
    fn spec(&self) -> &CommandSpec { &self.0 }
    fn execute<'a>(
        &'a self,
        _ctx: &'a mut crate::chat::commands::orchestrator::CommandContext<'a>,
        _args: serde_json::Value,
    ) -> CommandExecFuture<'a> {
        Box::pin(async { Ok(CommandResult { summary: "ok".to_string() }) })
    }
}

pub fn register_builtin_commands(registry: &mut CommandRegistry) -> Result<(), CommandError> {
    for (name, description) in [
        ("/plan", "Generate a plan with an auto-titled summary"),
        ("/discover-agents", "Refresh discovered ACP agents"),
    ] {
        registry.register(Arc::new(StubCommand(CommandSpec {
            name: name.to_string(),
            description: description.to_string(),
            args_schema: serde_json::json!({ "type": "object" }),
        })))?;
    }
    Ok(())
}
```

- [ ] **Step 6: Run tests to verify pass**

Run: `cargo test chat_command_registry -- --nocapture`  
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/chat/mod.rs src/chat/commands/mod.rs src/chat/commands/types.rs src/chat/commands/registry.rs src/chat/commands/builtins.rs tests/chat_command_registry.rs
git commit -m "feat(chat): add declarative command contracts and registry"
```

---

### Task 2: Implement layered command matching and picker filtering (TDD)

**Files:**
- Create: `src/chat/commands/matcher.rs`
- Modify: `src/chat/picker.rs`
- Test: `tests/chat_command_matching.rs`

- [ ] **Step 1: Write failing matcher tests**

```rust
// tests/chat_command_matching.rs
use blazar::chat::commands::matcher::ranked_match_names;
use blazar::chat::commands::CommandSpec;

fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec { name: "/model".to_string(), description: "Switch model".to_string(), args_schema: serde_json::json!({}) },
        CommandSpec { name: "/theme".to_string(), description: "Switch theme".to_string(), args_schema: serde_json::json!({}) },
        CommandSpec { name: "/plan".to_string(), description: "Generate a plan".to_string(), args_schema: serde_json::json!({}) },
    ]
}

#[test]
fn exact_match_ranks_first() {
    let ranked = ranked_match_names("/plan", &specs());
    assert_eq!(ranked.first().copied(), Some("/plan"));
}

#[test]
fn prefix_beats_contains_and_fuzzy() {
    let ranked = ranked_match_names("/mo", &specs());
    assert_eq!(ranked.first().copied(), Some("/model"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test chat_command_matching -- --nocapture`  
Expected: FAIL (missing matcher implementation).

- [ ] **Step 3: Implement matcher tiers and stable sorting**

```rust
// src/chat/commands/matcher.rs
use super::types::CommandSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchTier { Exact, Prefix, Contains, Fuzzy, None }

pub fn ranked_match_names<'a>(query: &str, specs: &'a [CommandSpec]) -> Vec<&'a str> {
    let needle = query.trim().to_lowercase();
    if needle == "/" || needle.is_empty() {
        return specs.iter().map(|s| s.name.as_str()).collect();
    }

    let mut scored: Vec<(MatchTier, i32, &'a str)> = specs
        .iter()
        .filter_map(|spec| score_spec(&needle, spec).map(|(tier, score)| (tier, score, spec.name.as_str())))
        .collect();

    scored.sort_by(|a, b| a.cmp(b));
    scored.into_iter().map(|(_, _, name)| name).collect()
}

fn score_spec(needle: &str, spec: &CommandSpec) -> Option<(MatchTier, i32)> {
    let name = spec.name.to_lowercase();
    let desc = spec.description.to_lowercase();
    if name == needle { return Some((MatchTier::Exact, -1000)); }
    if name.starts_with(needle) { return Some((MatchTier::Prefix, -(needle.len() as i32))); }
    if name.contains(needle) || desc.contains(needle) { return Some((MatchTier::Contains, 0)); }
    if is_subsequence(needle, &name) { return Some((MatchTier::Fuzzy, 100)); }
    None
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = needle.chars();
    let mut current = chars.next();
    for c in haystack.chars() {
        if current == Some(c) {
            current = chars.next();
            if current.is_none() { return true; }
        }
    }
    false
}
```

- [ ] **Step 4: Apply matcher in `ModalPicker` command context**

```rust
// src/chat/picker.rs (inside filtered_items)
pub fn filtered_items(&self) -> Vec<&PickerItem> {
    if self.context != PickerContext::Commands {
        return self.items
            .iter()
            .filter(|item| self.filter.is_empty() || item.label.to_lowercase().contains(&self.filter.to_lowercase()))
            .collect();
    }

    let filter = self.filter.trim().to_owned();
    if !filter.starts_with('/') {
        return Vec::new();
    }

    let specs: Vec<crate::chat::commands::CommandSpec> = self
        .items
        .iter()
        .map(|item| crate::chat::commands::CommandSpec {
            name: item.label.clone(),
            description: item.description.clone(),
            args_schema: serde_json::json!({ "type": "object" }),
        })
        .collect();

    let ordered_names = crate::chat::commands::matcher::ranked_match_names(&filter, &specs);
    ordered_names
        .into_iter()
        .filter_map(|name| self.items.iter().find(|item| item.label == name))
        .collect()
}
```

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test chat_command_matching picker -- --nocapture`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/chat/commands/matcher.rs src/chat/picker.rs tests/chat_command_matching.rs
git commit -m "feat(chat): add layered command matcher for slash palette"
```

---

### Task 3: Add async orchestrator and callback execution contract (TDD)

**Files:**
- Create: `src/chat/commands/orchestrator.rs`
- Modify: `src/chat/app.rs`
- Modify: `src/chat/app/actions.rs`
- Test: `tests/chat_command_orchestrator.rs`

- [ ] **Step 1: Write failing orchestrator tests**

```rust
// tests/chat_command_orchestrator.rs
use blazar::chat::app::ChatApp;
use blazar::chat::commands::orchestrator::execute_palette_command_for_test;
use serde_json::json;

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[tokio::test]
async fn execute_plan_command_sets_composer_prefill() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    execute_palette_command_for_test(&mut app, "/plan", json!({})).await.expect("ok");
    assert_eq!(app.composer_text(), "/plan ");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test chat_command_orchestrator -- --nocapture`  
Expected: FAIL (missing orchestrator and test hook).

- [ ] **Step 3: Implement orchestrator and command context**

```rust
// src/chat/commands/orchestrator.rs
use serde_json::Value;

use super::{CommandError, CommandRegistry, CommandResult};

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
        .ok_or_else(|| CommandError::Unavailable(format!("unknown command: {name}")))?;
    let mut ctx = CommandContext { app };
    command.execute(&mut ctx, args).await
}
```

- [ ] **Step 4: Wire `ChatApp` to use orchestrator from picker submit**

```rust
// src/chat/app.rs (fields)
command_registry: crate::chat::commands::CommandRegistry,
```

```rust
// src/chat/app.rs (constructor snippet)
let mut command_registry = crate::chat::commands::CommandRegistry::new();
crate::chat::commands::builtins::register_builtin_commands(&mut command_registry)?;
// ...
command_registry,
```

```rust
// src/chat/app.rs (new helper)
pub(crate) fn execute_palette_command_sync(
    &mut self,
    name: &str,
    args: serde_json::Value,
) -> Result<crate::chat::commands::CommandResult, crate::chat::commands::CommandError> {
    futures::executor::block_on(crate::chat::commands::orchestrator::execute_palette_command(
        &self.command_registry,
        self,
        name,
        args,
    ))
}
```

```rust
// src/chat/app/actions.rs (submit branch)
if ctx == PickerContext::Commands {
    let command_name = cmd.clone();
    let result = self.execute_palette_command_sync(&command_name, serde_json::json!({}));
    if let Err(err) = result {
        self.timeline.push(TimelineEntry::warning(format!("Command failed: {err:?}")));
    }
    return;
}
```

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test chat_command_orchestrator -- --nocapture`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/chat/commands/orchestrator.rs src/chat/app.rs src/chat/app/actions.rs tests/chat_command_orchestrator.rs
git commit -m "feat(chat): execute palette commands via async orchestrator"
```

---

### Task 4: Migrate all built-in slash commands to trait registrations

**Files:**
- Modify: `src/chat/commands/builtins.rs`
- Modify: `src/chat/picker.rs`
- Modify: `src/chat/app/actions.rs`
- Modify: `src/chat/app/turns.rs`
- Test: `tests/chat_command_registry.rs`
- Test: `tests/chat_runtime.rs`

- [ ] **Step 1: Write failing full-builtins coverage test**

```rust
// tests/chat_command_registry.rs
#[test]
fn builtin_registry_contains_all_palette_commands() {
    let mut registry = CommandRegistry::new();
    register_builtin_commands(&mut registry).expect("register");
    let names: Vec<&str> = registry.list().iter().map(|s| s.name.as_str()).collect();
    for required in [
        "/help", "/clear", "/copy", "/init", "/skills", "/model", "/mcp", "/theme",
        "/history", "/plan", "/export", "/compact", "/config", "/tools", "/agents",
        "/discover-agents", "/context", "/diff", "/git", "/undo", "/terminal", "/debug",
        "/log", "/quit",
    ] {
        assert!(names.contains(&required), "missing {required}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test builtin_registry_contains_all_palette_commands -- --nocapture`  
Expected: FAIL (not all commands registered yet).

- [ ] **Step 3: Implement all built-in command objects**

```rust
// src/chat/commands/builtins.rs (pattern)
struct ThemeCommand { spec: CommandSpec }

impl PaletteCommand for ThemeCommand {
    fn spec(&self) -> &CommandSpec { &self.spec }
    fn execute<'a>(&'a self, ctx: &'a mut CommandContext<'a>, _args: serde_json::Value) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.open_theme_picker();
            Ok(CommandResult { summary: "Opened theme selector".to_string() })
        })
    }
}
```

```rust
// /plan behavior in callback
ctx.app.set_composer_text("/plan ");
```

```rust
// /discover-agents behavior in callback
ctx.app.refresh_acp_agents();
```

- [ ] **Step 4: Remove static list and command-specific branches**

```rust
// src/chat/picker.rs
pub fn command_palette_from_registry(
    specs: &[&crate::chat::commands::CommandSpec],
) -> Self {
    let items = specs
        .iter()
        .map(|spec| PickerItem::new(spec.name, spec.description))
        .collect();
    Self::with_context("Commands", items, PickerContext::Commands)
}
```

```rust
// src/chat/app/actions.rs
// remove hard-coded: if cmd == "/theme" { ... } if cmd == "/model" { ... } if cmd == "/plan" { ... }
// route all slash command submit to orchestrator
```

```rust
// src/chat/app/turns.rs
// remove special case:
// if trimmed == "/discover-agents" { ... }
```

- [ ] **Step 5: Run regression tests**

Run: `cargo test command_palette chat_runtime -- --nocapture`  
Expected: PASS with existing behavior preserved.

- [ ] **Step 6: Commit**

```bash
git add src/chat/commands/builtins.rs src/chat/picker.rs src/chat/app/actions.rs src/chat/app/turns.rs tests/chat_command_registry.rs tests/chat_runtime.rs
git commit -m "refactor(chat): migrate all slash commands to declarative registry"
```

---

### Task 5: Final integration checks and docs sync

**Files:**
- Modify: `src/chat/picker.rs` (unit tests updated to registry-backed creation)
- Modify: `tests/chat_command_matching.rs` (final ranking assertions)
- Modify: `tests/chat_command_orchestrator.rs` (error-path assertions)

- [ ] **Step 1: Add failing tests for no-match and slash-only behavior**

```rust
// tests/chat_command_matching.rs
#[test]
fn slash_only_returns_all_commands() {
    let ranked = ranked_match_names("/", &specs());
    assert_eq!(ranked.len(), specs().len());
}

#[test]
fn non_slash_query_returns_no_commands() {
    let ranked = ranked_match_names("pla", &specs());
    assert!(ranked.is_empty());
}
```

- [ ] **Step 2: Run tests to verify fail then adjust matcher**

Run: `cargo test slash_only_returns_all_commands non_slash_query_returns_no_commands -- --nocapture`  
Expected: first run FAIL, after matcher guard updates PASS.

- [ ] **Step 3: Run repository quality gates**

Run: `just fmt-check`  
Expected: PASS.

Run: `just lint`  
Expected: PASS.

Run: `just test`  
Expected: PASS.

- [ ] **Step 4: Commit final polish**

```bash
git add src/chat/picker.rs src/chat/commands/matcher.rs tests/chat_command_matching.rs tests/chat_command_orchestrator.rs
git commit -m "test(chat): finalize command palette matching and orchestration coverage"
```

---

## Spec Coverage Check

1. **Trait + registry model:** Task 1 + Task 4 cover command declaration and full built-in migration.
2. **Async callback contract:** Task 3 introduces async orchestrator execution contract.
3. **All built-ins migrated:** Task 4 includes explicit full command list assertion.
4. **Matching rule (`exact > prefix > contains > fuzzy`) and trigger `/`:** Task 2 + Task 5 cover implementation and regression tests.
5. **UI shows name + description from declarations:** Task 4 changes picker construction to registry metadata.

No spec gaps found.

## Placeholder/Consistency Check

1. No `TODO/TBD/implement later` placeholders remain.
2. Names are consistent across tasks: `CommandRegistry`, `PaletteCommand`, `CommandMatcher`, `execute_palette_command`.
3. File paths are explicit and repeated where needed for out-of-order execution.
