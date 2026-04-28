# Command Plugin System Design

## Problem

All 24 commands are registered in a single `builtins.rs` file. 20 of them are `ForwardCommand` — they forward the command name as text to the agent, wasting tokens on deterministic operations that should execute locally. There is no extension point for adding commands at runtime.

## Goals

1. **Modular**: each command is a self-contained plugin in its own file
2. **Auto-registered**: built-in plugins use `inventory` crate for compile-time discovery (like Linux `module_init`)
3. **Extensible**: runtime registration API for future external plugins
4. **Local execution**: every command executes locally, no agent forwarding
5. **Categorized**: commands have categories (UI / Workspace / Agent / System)

## Architecture

### File Layout

```
src/chat/commands/
├── mod.rs                  # public exports
├── plugin.rs               # CommandPlugin trait, PluginMeta, PluginContext, PluginRegistration
├── registry.rs             # PluginRegistry (inventory built-in + runtime extension)
├── orchestrator.rs         # execution engine (adapts to new trait)
├── matcher.rs              # fuzzy matching (unchanged)
└── builtins/
    ├── mod.rs              # inventory::collect! gathering point + re-exports
    ├── help.rs             # /help
    ├── clear.rs            # /clear
    ├── copy.rs             # /copy
    ├── init.rs             # /init
    ├── skills.rs           # /skills
    ├── model.rs            # /model
    ├── mcp.rs              # /mcp
    ├── theme.rs            # /theme
    ├── history.rs          # /history
    ├── plan.rs             # /plan
    ├── export.rs           # /export
    ├── compact.rs          # /compact
    ├── config.rs           # /config
    ├── tools.rs            # /tools
    ├── agents.rs           # /agents
    ├── discover.rs         # /discover-agents
    ├── context.rs          # /context
    ├── diff.rs             # /diff
    ├── git.rs              # /git
    ├── undo.rs             # /undo
    ├── terminal.rs         # /terminal
    ├── debug.rs            # /debug
    ├── log.rs              # /log
    └── quit.rs             # /quit
```

### Core Types (`plugin.rs`)

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Plugin execution future.
pub type PluginFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PluginResult, PluginError>> + Send + 'a>>;

/// Metadata for a command plugin.
#[derive(Debug, Clone)]
pub struct PluginMeta {
    pub name: &'static str,
    pub description: &'static str,
    pub category: Category,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    UI,
    Workspace,
    Agent,
    System,
}

/// Result returned by a successful plugin execution.
#[derive(Debug, Clone)]
pub struct PluginResult {
    pub summary: String,
}

/// Errors that plugins can return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    InvalidArgs(String),
    Unavailable(String),
    ExecutionFailed(String),
}

/// The main plugin trait. Each command implements this.
pub trait CommandPlugin: Send + Sync {
    fn meta(&self) -> PluginMeta;

    fn execute<'a>(
        &'a self,
        ctx: &'a mut PluginContext<'a>,
        args: serde_json::Value,
    ) -> PluginFuture<'a>;
}

/// Context provided to plugins during execution.
pub struct PluginContext<'a> {
    pub app: &'a mut crate::chat::app::ChatApp,
}

/// Registration entry collected by inventory.
pub struct PluginRegistration {
    pub factory: fn() -> Arc<dyn CommandPlugin>,
}

inventory::collect!(PluginRegistration);
```

### Registry (`registry.rs`)

```rust
pub struct PluginRegistry {
    ordered: Vec<Arc<dyn CommandPlugin>>,
    by_name: HashMap<String, Arc<dyn CommandPlugin>>,
}

impl PluginRegistry {
    /// Create registry with all inventory-registered built-in plugins.
    pub fn with_builtins() -> Self {
        let mut reg = Self::default();
        for entry in inventory::iter::<PluginRegistration> {
            let plugin = (entry.factory)();
            reg.register(plugin).ok();
        }
        reg
    }

    /// Runtime extension point for external plugins.
    pub fn register_external(&mut self, plugin: Arc<dyn CommandPlugin>) -> Result<(), PluginError> {
        self.register(plugin)
    }
}
```

### Plugin File Template (e.g. `builtins/quit.rs`)

```rust
use std::sync::Arc;
use crate::chat::commands::plugin::*;

struct QuitPlugin;

impl CommandPlugin for QuitPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "/quit",
            description: "Exit Blazar",
            category: Category::UI,
        }
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut PluginContext<'a>,
        _args: serde_json::Value,
    ) -> PluginFuture<'a> {
        Box::pin(async move {
            ctx.app.request_quit();
            Ok(PluginResult { summary: "Exiting Blazar".into() })
        })
    }
}

inventory::submit! {
    PluginRegistration { factory: || Arc::new(QuitPlugin) }
}
```

## Command Implementations

### UI Commands

| Command | Implementation |
|---------|---------------|
| `/quit` | Set `should_quit = true` |
| `/clear` | Clear `timeline` vec and `messages` vec, reset scroll |
| `/debug` | Toggle debug overlay flag, push timeline info entry |
| `/help` | Iterate registry, format name + description as timeline entry |
| `/theme` | Open theme picker (`app.open_theme_picker()`) |
| `/model` | Open model picker (`app.open_model_picker()`) |
| `/log` | Read last 50 lines from log file, push as timeline entry |

### Workspace Commands

| Command | Implementation |
|---------|---------------|
| `/copy` | Find last assistant message, copy to system clipboard via `arboard` crate |
| `/diff` | Run `git diff` in workspace, display output as timeline code block |
| `/git` | Run `git status` + `git branch`, display as timeline entry |
| `/undo` | Track last modified files in agent state, run `git checkout -- <file>` |
| `/terminal` | Push timeline entry with workspace path and `cd` hint |
| `/init` | Write template `blazar-instructions.md` to workspace root |
| `/export` | Export messages as JSON (+ markdown companion) to workspace |
| `/config` | Display current config values or config file path |

### Agent Commands

| Command | Implementation |
|---------|---------------|
| `/context` | Read `context_usage` from app state, format token counts |
| `/tools` | List tools from agent runtime, format as timeline entry |
| `/agents` | List active background agents from agent state |
| `/skills` | List loaded skills from config/agent |
| `/compact` | Send summarization request to LLM, replace old messages with summary |
| `/plan` | Set composer text to "/plan " |
| `/discover-agents` | Trigger ACP agent discovery refresh |
| `/history` | Format message history as timeline entries |

### System Commands

| Command | Implementation |
|---------|---------------|
| `/mcp` | Display MCP server connection status |

## Dependencies

- **`inventory`** crate: compile-time plugin collection (already used pattern in Rust ecosystem)
- **`arboard`** crate: cross-platform clipboard access (for `/copy`)
- No new dependencies for other commands (git operations via `std::process::Command`)

## Migration

1. Rename old `PaletteCommand` → adapt to `CommandPlugin` trait
2. Replace `CommandRegistry` internals with `PluginRegistry`
3. Move each command from `builtins.rs` into its own file under `builtins/`
4. Remove `ForwardCommand` entirely
5. Update `orchestrator.rs` to use new types
6. Update `ChatApp` to expose needed methods (e.g. `request_quit()`, `clear_conversation()`)

## Backward Compatibility

- Command names unchanged (all `/xxx` preserved)
- Picker, matcher, keyboard shortcuts unchanged
- `CommandContext` renamed to `PluginContext` (internal only)

## Testing

- Each plugin gets unit tests in its own file
- Registry tests: auto-discovery, duplicate detection, find/list
- Integration: `execute_palette_command_for_test` updated for new types
