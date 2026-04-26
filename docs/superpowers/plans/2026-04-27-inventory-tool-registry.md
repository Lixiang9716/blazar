# Inventory Tool Registry — Implementation Plan

> **Agentic execution note:** This plan is designed for step-by-step execution.
> Each task produces a compilable, testable checkpoint.

## Problem

Built-in tools are registered via hand-maintained lists in two call sites.
Adding or removing a tool requires editing both. The approved spec uses
`inventory` to decentralize registration behind profile-aware descriptors.

**Spec:** `docs/superpowers/specs/2026-04-27-inventory-tool-registry-design.md`

## Tasks

### Task 1 — add-inventory-dep-and-core-types

**Files:** `Cargo.toml`, `src/agent/tools/mod_impl.inc`

1. Add `inventory = "0.3"` to `[dependencies]` in `Cargo.toml`
2. In `mod_impl.inc`, after existing imports, add:
   - `use crate::provider::LlmProvider;`
   - `use std::sync::Arc;`
   - `ToolBuildProfile` enum (`MainRuntime`, `SubAgent`)
   - `BuiltinToolProfiles` enum (`MainOnly`, `SubAgentOnly`, `Both`) with `fn includes(profile)` method
   - `ToolBuildContext` struct (public fields: `workspace_root`, `provider`, `model`)
   - `BuiltinToolDescriptor` struct (`name`, `profiles`, `build`)
   - `inventory::collect!(BuiltinToolDescriptor);`
   - Internal `fn collect_and_build_builtins(descriptors, ctx) -> Result<Vec<Box<dyn Tool>>, String>` that filters→sorts→dedup→builds→validates
   - Public `fn register_builtin_tools(ctx: &ToolBuildContext, profile: ToolBuildProfile) -> Result<ToolRegistry, String>` wrapping inventory::iter + collect_and_build_builtins
3. Verify: `cargo build` succeeds (no descriptors submitted yet, empty iteration)

**Checkpoint:** Compiles. `just test` passes. No behavioral change.

### Task 2 — add-tool-descriptors

**Files:** `src/agent/tools/read_file.rs`, `write_file.rs`, `list_dir.rs`, `bash.rs`, `vet.rs`, `agent.rs`

For each module, add an `inventory::submit!` block with the appropriate profile:

| Module | `BuiltinToolDescriptor.name` | Profile |
|--------|------------------------------|---------|
| read_file.rs | `"read_file"` | Both |
| write_file.rs | `"write_file"` | Both |
| list_dir.rs | `"list_dir"` | Both |
| bash.rs | `"bash"` | Both |
| vet.rs | `"vet"` | MainOnly |
| agent.rs | `"sub_agent"` | MainOnly |

For `agent.rs`, define `const AGENT_TOOL_NAME` and `const AGENT_TOOL_DESCRIPTION` so the descriptor and `AgentTool::new()` share one source of truth.

**Checkpoint:** Compiles. `just test` passes. No behavioral change yet (old assembly sites still active).

### Task 3 — rewrite-main-runtime-assembly

**File:** `src/agent/runtime.rs`

1. Replace body of `build_tool_registry()`:
   - Create `ToolBuildContext { workspace_root, provider, model }`
   - Call `register_builtin_tools(&ctx, ToolBuildProfile::MainRuntime)?`
   - Call `register_acp_tools(&mut tools, workspace_root)?`
   - Return `Ok(tools)`
2. Remove now-unused per-tool imports: `ReadFileTool`, `WriteFileTool`, `ListDirTool`, `BashTool`, `VetTool`, `AgentTool`
3. Add import for `register_builtin_tools`, `ToolBuildContext`, `ToolBuildProfile`

**Checkpoint:** `cargo build` + `just test` passes. Main runtime uses inventory path.

### Task 4 — rewrite-sub-agent-assembly

**File:** `src/agent/tools/agent.rs`

1. In `AgentTool::execute()`, replace hand-written 4-tool registration:
   ```rust
   let tools = match register_builtin_tools(
       &ToolBuildContext { workspace_root: self.workspace_root.clone(), provider: Arc::clone(&self.provider), model: self.model.clone() },
       ToolBuildProfile::SubAgent,
   ) {
       Ok(t) => t,
       Err(e) => return ToolResult::failure(format!("sub-agent tool assembly failed: {e}")),
   };
   ```
2. Remove direct imports of `ReadFileTool`, `WriteFileTool`, `ListDirTool`, `BashTool`

**Checkpoint:** `cargo build` + `just test` passes. Sub-agent uses inventory path.

### Task 5 — add-assembly-tests-and-verify

**File:** `tests/agent_tools_inventory.rs`

Tests that exercise the public API:

1. `main_runtime_provides_expected_tools` — assert exact name set: `["bash", "list_dir", "read_file", "sub_agent", "vet", "write_file"]`
2. `sub_agent_provides_expected_tools` — assert exact name set: `["bash", "list_dir", "read_file", "write_file"]`
3. `tools_are_sorted_by_name` — verify specs() returns alphabetical order

For negative cases (duplicate names, name mismatch), test the internal `collect_and_build_builtins` with synthetic descriptor slices — **not** via `inventory::submit!` (which would pollute all tests in the binary).

4. `duplicate_builtin_name_is_rejected` — pass two descriptors with same name → Err
5. `name_mismatch_is_rejected` — pass descriptor with name "x" but build fn returns tool with spec name "y" → Err

**Final verification:**
```bash
just fmt-check && just lint && just test
```

**Checkpoint:** All tests pass. Feature complete.
