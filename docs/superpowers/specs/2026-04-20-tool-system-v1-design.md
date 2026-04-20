# Tool System V1 Design

**Date:** 2026-04-20
**Status:** Approved

## Problem

Blazar has a hardened agent runtime (Phase 1) but the agent can only generate text — it cannot read files, run commands, or make changes. Without tools, Blazar is a chatbot, not a coding assistant.

## Design Goals

1. Let the model execute real actions (shell commands, file I/O) during a turn.
2. Keep Blazar-owned state; tool results flow through the existing event/timeline system.
3. Make the tool interface extensible for Phase 5 MCP dynamic tools.
4. Borrow proven patterns from Codex CLI (I/O capping, shell abstraction, multi-turn tool loop).
5. Ship a minimal set that makes the agent genuinely useful.

## Research: Codex CLI Tool Architecture

Key patterns borrowed from `openai/codex` (codex-rs):

| Pattern | Codex Implementation | Blazar Adaptation |
|---------|---------------------|-------------------|
| Tool spec | `ToolSpec` enum + `ToolDefinition` struct | `Tool` trait + `ToolSpec` struct |
| Parameter schema | Type-safe `JsonSchema` builders | `serde_json::Value` JSON Schema |
| Shell execution | `Shell` struct with auto-detect + `derive_exec_args` | `ShellConfig` with auto-detect |
| I/O capping | `EXEC_OUTPUT_MAX_BYTES` (8KB) + `read_capped()` | Same 8KB cap with truncation flag |
| Timeout | `ExecExpiration::Timeout(Duration)` default 10s | 30s default timeout |
| Process cleanup | `kill_child_process_group()` on timeout | Same pattern via `nix::sys::signal` |
| Tool results | Structured JSON with `exit_code`, `wall_time`, `output` | `ToolResult` with exit_code + truncation |
| Agent loop | Multi-turn: tool call → execute → result → model continues | Same multi-turn loop |

## Scope

### In scope (V1)

- Tool trait and registry
- 4 built-in tools: `bash`, `read_file`, `write_file`, `list_dir`
- Multi-turn agent loop (tool call → execute → feed result → continue)
- Provider interface upgrade (messages + tools)
- Tool call display in TUI timeline (inline collapsible)
- Shell abstraction with I/O cap and timeout

### Out of scope (deferred)

- Permission/approval system (Phase 3)
- MCP dynamic tools (Phase 5)
- Sandboxing/filesystem isolation (Phase 3)
- Parallel tool dispatch (future optimization)
- Tool search/suggest (Codex pattern, deferred until tool count justifies it)

## Architecture

### Core Types

```rust
// src/agent/tools/mod.rs

/// Specification sent to the model so it knows what tools are available.
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,  // JSON Schema object
}

/// Result of executing a tool.
pub struct ToolResult {
    pub output: String,
    pub exit_code: Option<i32>,
    pub is_error: bool,
    pub output_truncated: bool,
}

/// The tool interface. Each built-in tool implements this trait.
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn execute(&self, args: serde_json::Value) -> ToolResult;
}

/// Holds all available tools and dispatches by name.
/// Owned by `AgentRuntime` and passed to the runtime loop.
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new(workspace_root: PathBuf) -> Self;
    pub fn register(&mut self, tool: Box<dyn Tool>);
    pub fn get(&self, name: &str) -> Option<&dyn Tool>;
    pub fn specs(&self) -> Vec<ToolSpec>;
}
```

Tools that need filesystem access receive `workspace_root: PathBuf` at construction time (injected by `ToolRegistry::new()`). All path-based tools resolve relative paths against this root.
```

### Built-in Tools

#### `bash`

| Field | Value |
|-------|-------|
| Parameters | `command: String` (required), `timeout_secs: Option<u64>` (default 30) |
| Execution | Spawn via detected shell (`bash -c`, `sh -c`), capture stdout+stderr |
| Output cap | 8KB max (`MAX_OUTPUT_BYTES = 8192`), truncated with `[output truncated]` marker |
| Timeout | Kill process group after timeout, return timeout error |
| Result | `output`, `exit_code`, `is_error` (exit != 0), `output_truncated` |

Shell auto-detection order: `$SHELL` env → `/bin/bash` → `/bin/sh`.

Process group management: spawn with `setsid`, kill group on timeout via `killpg(SIGTERM)` → 2s drain → `killpg(SIGKILL)`.

#### `read_file`

| Field | Value |
|-------|-------|
| Parameters | `path: String` (required) |
| Output cap | 100KB max; files larger return error |
| Result | File content as string, or error message |

Path is resolved relative to the workspace root (repo_path from ChatApp).

#### `write_file`

| Field | Value |
|-------|-------|
| Parameters | `path: String` (required), `content: String` (required) |
| Behavior | Create parent directories if needed, write content |
| Result | Confirmation message with bytes written, or error |

Path is resolved relative to workspace root.

#### `list_dir`

| Field | Value |
|-------|-------|
| Parameters | `path: String` (required, default ".") |
| Behavior | List entries up to 2 levels deep, show type (file/dir) |
| Output cap | Max 200 entries; truncated with count |
| Result | Formatted directory listing |

### Provider Interface Upgrade

Current:
```rust
pub trait LlmProvider: Send {
    fn stream_turn(&self, prompt: &str, tx: Sender<ProviderEvent>);
}
```

New:
```rust
pub trait LlmProvider: Send {
    fn stream_turn(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    );
}
```

Where `Message` is:
```rust
pub enum Message {
    User { content: String },
    Assistant { content: String },
    ToolCall { id: String, name: String, arguments: String },
    ToolResult { tool_call_id: String, output: String, is_error: bool },
}
```

`ProviderEvent` gains a new variant:
```rust
pub enum ProviderEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolCall { id: String, name: String, arguments: String },
    TurnComplete,
    Error(String),
}
```

### Agent Loop (Multi-Turn Tool Use)

The current `run_turn()` is single-shot: prompt → stream → done.

New loop:
```
fn run_turn(messages, tools, provider, event_tx, cancel_flag):
    loop:
        provider.stream_turn(messages, tools, event_tx)
        
        collect events:
            TextDelta → relay to UI
            ToolCall → collect into pending_calls
            TurnComplete → break if no pending_calls
        
        if pending_calls is empty:
            break  // model finished without tool use
        
        for call in pending_calls:
            emit ToolCallStarted to UI
            result = registry.execute(call.name, call.arguments)
            emit ToolCallCompleted to UI
            append ToolCall + ToolResult to messages
        
        // loop: model sees tool results and continues
    
    emit TurnComplete
```

Key: maximum tool iterations per turn = 10 (safety limit to prevent infinite loops).

### Protocol Extensions

New `AgentEvent` variants for the UI:
```rust
pub enum AgentEvent {
    // existing...
    TurnStarted { turn_id: String },
    TextDelta { text: String },
    ThinkingDelta { text: String },
    TurnComplete,
    TurnFailed { error: String },
    // new:
    ToolCallStarted { call_id: String, tool_name: String, arguments: String },
    ToolCallCompleted { call_id: String, output: String, is_error: bool },
}
```

### TUI Timeline Display

New `EntryKind` variant:
```rust
EntryKind::ToolCall {
    tool_name: String,
    arguments_summary: String,
    output_preview: String,
    full_output: String,
    is_error: bool,
    collapsed: bool,
}
```

Rendering:
- **Collapsed** (default): `🔧 bash: ls -la src/ ✓` or `🔧 read_file: src/main.rs ✗`
- **Expanded**: full output below the summary line
- Toggle: user can expand/collapse (future: arrow keys on focused entry)

### Module Layout

```
src/agent/tools/
    mod.rs          # Tool, ToolSpec, ToolResult, ToolRegistry
    bash.rs         # BashTool + ShellConfig
    read_file.rs    # ReadFileTool
    write_file.rs   # WriteFileTool
    list_dir.rs     # ListDirTool
```

Modified files:
- `src/agent/protocol.rs` — new AgentEvent variants
- `src/agent/runtime.rs` — multi-turn loop, ToolRegistry integration
- `src/agent/state.rs` — tool call state tracking
- `src/provider/mod.rs` — Message type, updated LlmProvider trait
- `src/provider/echo.rs` — updated for new trait signature
- `src/provider/siliconflow.rs` — tool_use support in API calls
- `src/chat/app.rs` — message history management, tool event handling
- `src/chat/model.rs` — EntryKind::ToolCall variant
- `src/chat/view/timeline.rs` — tool call rendering

### Testing Strategy

- **Unit tests per tool**: mock filesystem/shell, verify ToolResult fields
- **I/O cap test**: verify output truncation at 8KB boundary
- **Timeout test**: verify process kill on timeout
- **Registry tests**: register, lookup, list specs
- **Agent loop test**: mock provider that returns ToolCall events → verify execution → verify result fed back
- **Integration test**: full turn with EchoProvider that exercises tool protocol

Target: maintain 90%+ coverage on agent/tools module.

## Dependencies

New crate dependencies:
- `nix` (for `setsid`, `killpg` on Unix) — or use `std::os::unix::process::CommandExt`
- `serde_json` (already in use)
- No other new dependencies needed

## Risks

1. **Provider API breaking change**: `LlmProvider` trait signature changes, affecting both providers and all tests.
   - Mitigation: update all call sites in one commit.

2. **SiliconFlow tool_use support**: need to verify the API supports function calling.
   - Mitigation: EchoProvider as fallback for testing.

3. **Shell security**: bash execution in user's repo could be dangerous.
   - Mitigation: V1 auto-executes (user accepted this); Phase 3 adds permissions.
   - Workspace-root scoping prevents path traversal outside repo.
