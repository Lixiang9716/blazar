# Command Plugin System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace monolithic `builtins.rs` command registration with a plugin-style command system (compile-time inventory + runtime extension point), and implement all existing slash commands as local deterministic commands.

**Architecture:** Introduce a `CommandPlugin` abstraction and inventory-backed registration, keep command matching/orchestration stable, and split built-ins into one file per command. Add explicit `ChatApp` command-action methods so plugin behavior stays in app-owned state boundaries. Implement `/compact` as a local command that initiates a dedicated compaction turn through runtime instead of forwarding slash text.

**Tech Stack:** Rust, Tokio, `inventory`, `serde_json`, `std::process::Command`, existing `ChatApp` state/runtime ports.

---

## File Structure Map

- **Create:** `src/chat/commands/plugin.rs`  
  Owns plugin contracts: metadata, plugin trait, plugin context, inventory registration type.
- **Create:** `src/chat/commands/builtins/mod.rs`  
  Declares per-command modules; provides optional helper utilities reused by command plugins.
- **Create:** `src/chat/commands/builtins/{help,clear,copy,init,skills,model,mcp,theme,history,plan,export,compact,config,tools,agents,discover,context,diff,git,undo,terminal,debug,log,quit}.rs`  
  One plugin per command.
- **Modify:** `src/chat/commands/mod.rs`  
  Export new plugin interfaces, keep call sites simple.
- **Modify:** `src/chat/commands/types.rs`  
  Keep/bridge command error/result types if needed by existing test and orchestrator callers.
- **Modify:** `src/chat/commands/registry.rs`  
  Inventory-backed auto-register + runtime external registration.
- **Modify:** `src/chat/commands/orchestrator.rs`  
  Execute through `CommandPlugin`.
- **Modify:** `src/chat/commands/builtins.rs`  
  Remove (or reduce to compatibility shim) once split modules are live.
- **Modify:** `src/chat/app.rs`  
  Build registry using new plugin registration path.
- **Modify:** `src/chat/app/actions.rs`  
  Continue palette/selection behavior; ensure command invocations use plugin system.
- **Modify:** `src/chat/app/turns.rs`  
  Add compact turn construction and dispatch path.
- **Modify:** `src/chat/app/events.rs`  
  Handle compact turn completion by replacing historical messages with summary payload.
- **Modify:** `src/chat/runtime_port.rs` (only if required)  
  Keep boundary unchanged if possible; only extend if compact flow requires explicit API.
- **Modify:** `Cargo.toml`  
  Add `arboard` dependency for `/copy`.
- **Modify/Create tests:**  
  - `tests/chat_command_registry.rs`
  - `tests/chat_command_orchestrator.rs`
  - `tests/chat_command_matching.rs` (only if list/ranking expectations change)
  - `tests/unit/chat/app/tests_impl.inc`
  - `tests/chat_runtime.rs` (only if compact runtime lifecycle needs integration assertions)

---

### Task 1: Introduce plugin contracts and registry migration

**Files:**
- Create: `src/chat/commands/plugin.rs`
- Modify: `src/chat/commands/mod.rs`
- Modify: `src/chat/commands/registry.rs`
- Modify: `src/chat/commands/orchestrator.rs`
- Test: `tests/chat_command_registry.rs`, `tests/chat_command_orchestrator.rs`

- [ ] **Step 1: Write failing tests for inventory-backed registry shape**

```rust
// tests/chat_command_registry.rs
#[test]
fn builtin_registry_contains_all_palette_commands() {
    let registry = CommandRegistry::with_builtins();
    let names: Vec<&str> = registry.list().iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"/help"));
    assert!(names.contains(&"/quit"));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test --test chat_command_registry builtin_registry_contains_all_palette_commands -q`  
Expected: FAIL with `no function or associated item named 'with_builtins'`.

- [ ] **Step 3: Implement plugin contracts + registry bootstrap**

```rust
// src/chat/commands/plugin.rs
pub trait CommandPlugin: Send + Sync {
    fn spec(&self) -> CommandSpec;
    fn execute<'a>(&'a self, ctx: &'a mut CommandContext<'a>, args: Value) -> CommandExecFuture<'a>;
}

pub struct PluginRegistration {
    pub factory: fn() -> Arc<dyn CommandPlugin>,
}
inventory::collect!(PluginRegistration);
```

```rust
// src/chat/commands/registry.rs
pub fn with_builtins() -> Self {
    let mut registry = Self::new();
    for entry in inventory::iter::<PluginRegistration> {
        registry.register((entry.factory)())?;
    }
    registry
}
```

- [ ] **Step 4: Run focused tests to verify pass**

Run: `cargo test --test chat_command_registry -- --nocapture`  
Expected: PASS for registry tests.

- [ ] **Step 5: Commit**

```bash
git add src/chat/commands/{plugin.rs,mod.rs,registry.rs,orchestrator.rs} tests/chat_command_registry.rs tests/chat_command_orchestrator.rs
git commit -m "refactor(commands): introduce plugin contracts and inventory registry"
```

---

### Task 2: Split built-ins into per-command plugin modules

**Files:**
- Create: `src/chat/commands/builtins/mod.rs`
- Create: `src/chat/commands/builtins/{help,clear,model,theme,plan,discover,debug,quit}.rs`
- Modify/Delete: `src/chat/commands/builtins.rs`
- Test: `tests/chat_command_registry.rs`, `tests/chat_command_orchestrator.rs`

- [ ] **Step 1: Write failing tests for split registration**

```rust
// tests/chat_command_orchestrator.rs
#[tokio::test]
async fn execute_quit_command_sets_quit_flag() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    execute_palette_command_for_test(&mut app, "/quit", json!({})).await.expect("quit");
    assert!(app.should_quit());
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test --test chat_command_orchestrator execute_quit_command_sets_quit_flag -q`  
Expected: FAIL with unavailable `/quit` local behavior.

- [ ] **Step 3: Implement module-per-command plugins and inventory submissions**

```rust
// src/chat/commands/builtins/quit.rs
pub struct QuitCommand;
impl CommandPlugin for QuitCommand {
    fn spec(&self) -> CommandSpec { spec("/quit", "Exit Blazar") }
    fn execute<'a>(&'a self, ctx: &'a mut CommandContext<'a>, _args: Value) -> CommandExecFuture<'a> {
        Box::pin(async move {
            ctx.app.request_quit();
            Ok(CommandResult { summary: "Exiting Blazar".into() })
        })
    }
}
inventory::submit! { PluginRegistration { factory: || Arc::new(QuitCommand) } }
```

- [ ] **Step 4: Run split-command tests**

Run: `cargo test --test chat_command_registry --test chat_command_orchestrator -q`  
Expected: PASS; no `ForwardCommand`-based behavior for migrated commands.

- [ ] **Step 5: Commit**

```bash
git add src/chat/commands/builtins src/chat/commands/mod.rs tests/chat_command_registry.rs tests/chat_command_orchestrator.rs
git commit -m "refactor(commands): split built-ins into plugin modules"
```

---

### Task 3: Add ChatApp local command action methods (state-owned operations)

**Files:**
- Modify: `src/chat/app.rs`
- Modify: `src/chat/app/actions.rs`
- Modify: `src/chat/app/turns.rs`
- Test: `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Write failing tests for app-owned command actions**

```rust
#[test]
fn clear_conversation_resets_messages_and_keeps_banner() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    app.send_message_without_command_dispatch("hello");
    app.clear_conversation();
    assert!(app.messages().is_empty());
    assert!(!app.timeline().is_empty());
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test clear_conversation_resets_messages_and_keeps_banner -q`  
Expected: FAIL with missing `clear_conversation`.

- [ ] **Step 3: Implement explicit app methods used by plugins**

```rust
impl ChatApp {
    pub(crate) fn request_quit(&mut self) { self.should_quit = true; }

    pub(crate) fn clear_conversation(&mut self) {
        self.messages.clear();
        self.timeline.retain(|entry| entry.kind == EntryKind::Banner);
        self.scroll_offset = u16::MAX;
    }

    pub(crate) fn push_system_hint(&mut self, body: impl Into<String>) {
        self.timeline.push(TimelineEntry::hint(body));
        self.scroll_offset = u16::MAX;
    }
}
```

- [ ] **Step 4: Run app unit tests**

Run: `cargo test --test chat_runtime -q && cargo test --lib chat::app -q`  
Expected: PASS for app action tests.

- [ ] **Step 5: Commit**

```bash
git add src/chat/app.rs src/chat/app/actions.rs src/chat/app/turns.rs tests/unit/chat/app/tests_impl.inc
git commit -m "feat(chat): add app-owned local command action methods"
```

---

### Task 4: Implement remaining local informational commands

**Files:**
- Create: `src/chat/commands/builtins/{help,context,tools,agents,skills,history,config,mcp,terminal,log}.rs`
- Modify: `src/chat/commands/builtins/mod.rs`
- Test: `tests/chat_command_orchestrator.rs`

- [ ] **Step 1: Write failing orchestrator tests for local informational commands**

```rust
#[tokio::test]
async fn help_command_pushes_command_list_to_timeline() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    execute_palette_command_for_test(&mut app, "/help", json!({})).await.expect("help");
    assert!(app.timeline().iter().any(|e| e.body.contains("/model")));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test --test chat_command_orchestrator help_command_pushes_command_list_to_timeline -q`  
Expected: FAIL because `/help` does not write local informational timeline output yet.

- [ ] **Step 3: Implement local informational command plugins**

```rust
// src/chat/commands/builtins/help.rs
let specs = ctx.app.command_specs_for_help();
let body = specs.into_iter()
    .map(|s| format!("{} - {}", s.name, s.description))
    .collect::<Vec<_>>()
    .join("\n");
ctx.app.push_system_hint(body);
```

```rust
// src/chat/commands/builtins/context.rs
if let Some(usage) = ctx.app.context_usage_snapshot() {
    ctx.app.push_system_hint(format!("Context: {}/{} tokens", usage.used_tokens, usage.max_tokens));
} else {
    ctx.app.push_system_hint("Context usage is not available yet.");
}
```

- [ ] **Step 4: Run informational command tests**

Run: `cargo test --test chat_command_orchestrator -q`  
Expected: PASS for `/help`, `/context`, `/tools`, `/agents`, `/skills`, `/history`, `/config`, `/mcp`, `/terminal`, `/log`.

- [ ] **Step 5: Commit**

```bash
git add src/chat/commands/builtins tests/chat_command_orchestrator.rs
git commit -m "feat(commands): implement local informational command plugins"
```

---

### Task 5: Implement local workspace commands (`/copy`, `/init`, `/git`, `/diff`, `/undo`, `/export`)

**Files:**
- Modify: `Cargo.toml` (add `arboard`)
- Create: `src/chat/commands/builtins/{copy,init,git,diff,undo,export}.rs`
- Modify: `src/chat/commands/builtins/mod.rs`
- Modify: `src/chat/app.rs` (helpers for workspace path and snapshots)
- Test: `tests/chat_command_orchestrator.rs`, `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Write failing tests for workspace command side effects**

```rust
#[tokio::test]
async fn export_command_writes_json_snapshot_file() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    execute_palette_command_for_test(&mut app, "/export", json!({})).await.expect("export");
    assert!(app.timeline().iter().any(|e| e.body.contains(".json")));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test --test chat_command_orchestrator export_command_writes_json_snapshot_file -q`  
Expected: FAIL; export file not created by local plugin yet.

- [ ] **Step 3: Implement workspace command plugins**

```rust
// src/chat/commands/builtins/export.rs
let export = serde_json::to_string_pretty(&ctx.app.export_session_snapshot())?;
let file = ctx.app.workspace_root().join(format!("blazar-session-{}.json", timestamp));
std::fs::write(&file, export)?;
ctx.app.push_system_hint(format!("Exported session to {}", file.display()));
```

```rust
// src/chat/commands/builtins/copy.rs
let latest = ctx.app.latest_assistant_response().ok_or_else(|| CommandError::Unavailable("No assistant response to copy".into()))?;
let mut clipboard = arboard::Clipboard::new().map_err(|e| CommandError::ExecutionFailed(e.to_string()))?;
clipboard.set_text(latest).map_err(|e| CommandError::ExecutionFailed(e.to_string()))?;
```

- [ ] **Step 4: Run workspace command tests**

Run: `cargo test --test chat_command_orchestrator -q`  
Expected: PASS for workspace command scenarios; `/undo` handles empty/invalid targets with warning instead of panic.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/chat/commands/builtins src/chat/app.rs tests/chat_command_orchestrator.rs tests/unit/chat/app/tests_impl.inc
git commit -m "feat(commands): implement local workspace command plugins"
```

---

### Task 6: Implement `/compact` as local LLM summary compaction flow

**Files:**
- Modify: `src/chat/app.rs` (turn kinds/state for compaction)
- Modify: `src/chat/app/turns.rs` (build/dispatch compact prompt)
- Modify: `src/chat/app/events.rs` (apply compact result on completion)
- Create/Modify: `src/chat/commands/builtins/compact.rs`
- Test: `tests/unit/chat/app/tests_impl.inc`, `tests/chat_command_orchestrator.rs`

- [ ] **Step 1: Write failing tests for compaction dispatch + apply**

```rust
#[test]
fn compact_command_creates_compact_turn_kind() {
    let turn = build_pending_turn_for_mode("/compact", UserMode::Auto);
    assert_eq!(turn.user_text, "/compact");
    assert!(matches!(turn.dispatch, PendingDispatch::Runtime { kind: TurnKind::Compact, .. }));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test compact_command_creates_compact_turn_kind -q`  
Expected: FAIL because `TurnKind::Compact` and `/compact` dispatch path are not implemented.

- [ ] **Step 3: Implement compact turn path and finalize behavior**

```rust
// turns.rs
if trimmed == "/compact" {
    return PendingTurn {
        user_text: trimmed.to_owned(),
        dispatch: PendingDispatch::Runtime {
            runtime_prompt: build_compact_prompt(&self.messages),
            kind: TurnKind::Compact,
        },
        timeline_inserted: false,
    };
}
```

```rust
// events.rs (TurnComplete branch)
if self.active_turn_kind == Some(TurnKind::Compact) {
    self.finalize_compact_response();
}
```

- [ ] **Step 4: Run compaction tests**

Run: `cargo test --lib compact -- --nocapture`  
Expected: PASS; old messages replaced with summary envelope, timeline includes compact success hint.

- [ ] **Step 5: Commit**

```bash
git add src/chat/app.rs src/chat/app/turns.rs src/chat/app/events.rs src/chat/commands/builtins/compact.rs tests/unit/chat/app/tests_impl.inc tests/chat_command_orchestrator.rs
git commit -m "feat(commands): implement local LLM compaction workflow"
```

---

### Task 7: Remove forwarding path, finalize integration, and harden tests

**Files:**
- Modify/Delete: `src/chat/commands/builtins.rs` (remove `ForwardCommand`)
- Modify: `src/chat/commands/mod.rs`
- Modify: `tests/chat_command_registry.rs`
- Modify: `tests/chat_command_matching.rs`
- Modify: `tests/chat_command_orchestrator.rs`

- [ ] **Step 1: Write failing integration assertions that no command forwards slash text**

```rust
#[tokio::test]
async fn help_command_does_not_enqueue_user_slash_turn() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    execute_palette_command_for_test(&mut app, "/help", json!({})).await.expect("help");
    assert!(!app.timeline().iter().any(|e| e.body == "/help" && e.actor == Actor::User));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test --test chat_command_orchestrator help_command_does_not_enqueue_user_slash_turn -q`  
Expected: FAIL while any command still depends on forward-slash message dispatch.

- [ ] **Step 3: Remove forwarding-only code and wire complete local plugin set**

```rust
// builtins/mod.rs
pub mod help;
pub mod clear;
pub mod copy;
// ... all command modules declared here
```

```rust
// app.rs
let command_registry = crate::chat::commands::CommandRegistry::with_builtins();
```

- [ ] **Step 4: Run full command test suite**

Run: `cargo test --test chat_command_registry --test chat_command_matching --test chat_command_orchestrator -q`  
Expected: PASS; registry/matcher/orchestrator are all green with plugin architecture.

- [ ] **Step 5: Commit**

```bash
git add src/chat/commands src/chat/app.rs tests/chat_command_registry.rs tests/chat_command_matching.rs tests/chat_command_orchestrator.rs
git commit -m "refactor(commands): complete plugin migration and remove forwarding path"
```

---

### Task 8: End-to-end verification and branch hygiene

**Files:**
- Modify (if needed): any failing lint/test touch-ups from previous tasks only

- [ ] **Step 1: Run formatter**

Run: `just fmt-check`  
Expected: PASS with no formatting diffs.

- [ ] **Step 2: Run linter**

Run: `just lint`  
Expected: PASS with no clippy warnings.

- [ ] **Step 3: Run test suite**

Run: `just test`  
Expected: PASS with all existing tests and new command-plugin tests green.

- [ ] **Step 4: Run a manual smoke script**

Run:

```bash
cargo run --quiet
# In app:
# /help
# /model
# /theme
# /git
# /diff
# /export
# /compact
# /quit
```

Expected: Every command executes locally and produces deterministic timeline behavior.

- [ ] **Step 5: Final commit (if verification touch-ups were needed)**

```bash
git add -A
git commit -m "test(commands): stabilize plugin command integration"
```

---

## Self-Review Checklist (completed)

- **Spec coverage:** All spec sections are mapped to tasks (plugin contracts, inventory registration, local command implementations, compact flow, migration, tests).
- **Placeholder scan:** No TBD/TODO placeholders remain in implementation tasks.
- **Type consistency:** Plan consistently uses `CommandPlugin`, `PluginRegistration`, `CommandRegistry::with_builtins()`, and local `ChatApp` action methods.
