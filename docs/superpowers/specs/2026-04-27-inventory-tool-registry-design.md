# Inventory-Backed Built-in Tool Registration Design

**Date:** 2026-04-27
**Status:** Approved

## Problem

Blazar currently wires built-in agent tools through hand-maintained registration lists in more than one place.

Primary duplication points:

- `src/agent/runtime.rs` builds the main runtime tool set with explicit `tools.register(...)` calls
- `src/agent/tools/agent.rs` builds the sub-agent tool set with a second explicit built-in list

This creates three problems:

1. adding or removing a built-in tool requires editing multiple assembly sites
2. profile differences between main runtime and sub-agent are expressed by copying lists instead of by policy
3. built-in registration logic is coupled to runtime construction code even though ACP tools already remain a separate runtime source

Goal: decentralize built-in tool registration without changing ACP discovery into a compile-time mechanism and without moving core runtime state out of Blazar-owned types.

## Scope

### In scope

1. Introduce an `inventory`-backed registration path for compiled-in built-in tools
2. Add a build-context layer so built-ins can still receive runtime dependencies (`workspace_root`, provider, model)
3. Unify main-runtime and sub-agent built-in assembly behind shared profile filtering
4. Keep `ToolRegistry` as the single runtime aggregation point for both built-ins and ACP-discovered tools
5. Preserve current name-based lookup semantics and collision behavior

### Out of scope

1. Replacing ACP runtime discovery with compile-time registration
2. Adding separately shipped dynamic plugin loading
3. Refactoring capability wrappers or scheduler/resource-claim semantics
4. Changing tool execution behavior or user-visible tool contracts
5. Generalizing the same mechanism to chat commands in this phase

## Options considered

### A) `inventory` + factory descriptor layer (chosen)

Register static built-in descriptors with `inventory`, then build concrete tools at runtime from a Blazar-owned context object.

Pros:

- small, targeted adoption aligned with repository guidance
- preserves Blazar ownership of runtime state and wiring
- solves duplicated built-in registration without disturbing ACP discovery
- works naturally with tools that require runtime construction inputs

Cons:

- requires a small descriptor/build layer instead of directly registering tool instances
- `inventory` iteration order is not guaranteed and must be normalized explicitly

### B) `dyn-inventory`

Use an `inventory`-based helper crate oriented around dyn-compatible traits.

Pros:

- less macro boilerplate if the plugin unit is a dyn trait object

Cons:

- built-in tools in Blazar are better modeled as runtime-built values than static dyn instances
- still requires a factory-style abstraction once runtime context enters the picture
- introduces a less established abstraction without solving a core problem that plain `inventory` cannot solve

### C) Dynamic plugin systems (`dynamic-plugin`, `abi_stable`, `extism`, `libloading`)

Treat tool registration as an external plugin platform problem.

Pros:

- enables separately shipped plugins in future

Cons:

- much larger architecture shift than needed
- does not replace ACP protocol discovery
- adds ABI/runtime/packaging complexity unrelated to the current pain point

## Chosen design

### 1. Core registration types

Add a built-in registration layer centered on descriptors, not on tool instances.

```rust
pub enum ToolBuildProfile {
    MainRuntime,
    SubAgent,
}

pub struct ToolBuildContext {
    pub workspace_root: PathBuf,
    pub provider: Arc<dyn LlmProvider>,
    pub model: String,
    pub profile: ToolBuildProfile,
}

pub struct BuiltinToolDescriptor {
    pub name: &'static str,
    pub profiles: BuiltinToolProfiles,
    pub build: fn(&ToolBuildContext) -> Box<dyn Tool>,
}

inventory::collect!(BuiltinToolDescriptor);
```

Key point: `inventory` stores compile-time descriptors only. Blazar still constructs runtime tool values itself.

`BuiltinToolProfiles` can be a small enum/bitflag-style type expressing whether a descriptor applies to:

- main runtime only
- sub-agent only
- both

### 2. Registration contract is name-first

`ToolRegistry` already looks up tools by `ToolSpec::name`, so descriptor identity must be name-based as well.

That means:

1. every `BuiltinToolDescriptor` declares a canonical `name`
2. duplicate built-in descriptor names are an assembly error
3. after `build(...)`, the produced tool must advertise the same `ToolSpec::name`

This avoids drift between descriptor metadata and runtime tool behavior.

### 3. Determinism comes from names, not linker order

Because `inventory::iter` does not guarantee order, built-in collection must normalize before registration:

1. collect descriptors matching the requested profile
2. sort descriptors by `name`
3. reject duplicate names
4. build concrete tools
5. verify `descriptor.name == built_tool.spec().name`
6. register into `ToolRegistry`

This keeps behavior reproducible without inventing an additional ordering system that is not needed by current lookup semantics.

### 4. Runtime wiring

Main runtime assembly becomes:

1. create empty `ToolRegistry`
2. create `ToolBuildContext { profile: MainRuntime, ... }`
3. register compiled-in built-ins from `inventory`
4. append ACP-discovered tools through the existing runtime discovery path

Sub-agent assembly becomes:

1. create empty `ToolRegistry`
2. create `ToolBuildContext { profile: SubAgent, ... }`
3. register only descriptors allowed for the sub-agent profile
4. do not run ACP discovery

This removes the duplicated built-in lists while preserving the fact that ACP tools are a runtime protocol source, not a compile-time one.

### 5. ToolRegistry remains the runtime aggregation boundary

`ToolRegistry` continues to be the only runtime collection used by execution, lookup, specs, capability wrapping, and ACP collision checks.

This design does **not** turn `inventory` into a new runtime framework. `inventory` is only the assembly feed for compiled-in built-ins.

### 6. Expected migration shape

Each built-in tool module contributes its own descriptor near the tool implementation, for example conceptually:

```rust
fn build_read_file(ctx: &ToolBuildContext) -> Box<dyn Tool> {
    Box::new(ReadFileTool::new(ctx.workspace_root.clone()))
}

inventory::submit! {
    BuiltinToolDescriptor {
        name: "read_file",
        profiles: BuiltinToolProfiles::BOTH,
        build: build_read_file,
    }
}
```

Tools that need richer runtime inputs, such as `AgentTool`, keep receiving them through `ToolBuildContext` rather than through static globals or constructor side effects.

## Error handling

No silent fallback behavior is introduced.

1. **Duplicate built-in descriptor names**
   - fail fast during built-in assembly
   - do not silently overwrite

2. **Descriptor/tool name mismatch**
   - fail fast if `descriptor.name != built_tool.spec().name`
   - treat as assembly contract violation

3. **Profile mismatch**
   - normal filtering behavior, not an error

4. **ACP collisions**
   - preserve the current policy based on tool names
   - built-ins are assembled first
   - ACP registration continues to reject or skip name collisions as it does today

5. **Tool execution failures**
   - unchanged
   - remain inside `Tool::execute(...)` and existing runtime/capability plumbing

## Testing strategy

Add focused tests around the new assembly layer while preserving existing registry/runtime behavior tests.

### New tests

1. built-in descriptors are collected for the requested profile
2. descriptors are normalized by name before registration
3. duplicate built-in names fail fast
4. descriptor/tool name mismatch fails fast
5. sub-agent profile excludes tools that should not be available there
6. ACP collision behavior remains unchanged when built-ins are inventory-backed

### Existing behavior that should remain green

1. `ToolRegistry::get`, `specs`, and `execute` continue to work by tool name
2. capability wrappers still project registered tools the same way
3. runtime integration tests still see the same built-in tool surface for a given profile

## Rollout plan

1. introduce `ToolBuildContext`, `BuiltinToolDescriptor`, and the shared `register_builtin_tools(...)` helper
2. migrate the main runtime built-in assembly in `src/agent/runtime.rs`
3. migrate the sub-agent built-in assembly in `src/agent/tools/agent.rs`
4. leave ACP discovery/registration structurally unchanged and connect it after built-in registration
5. add focused assembly tests and run repository quality gates

## Risks and mitigations

1. **Descriptor drift from tool spec name**
   - enforce post-build equality check between descriptor name and `ToolSpec::name`

2. **Hidden dependence on previous manual registration order**
   - normalize by name and keep tests explicit about name-based lookup expectations

3. **Overreaching into ACP architecture**
   - keep ACP on the existing runtime discovery path and restrict `inventory` to compiled-in built-ins only

4. **Leaking runtime ownership into third-party registration code**
   - keep descriptors static and pass all runtime state through `ToolBuildContext`

## Success criteria

1. main runtime and sub-agent no longer maintain separate hand-written built-in registration lists
2. built-in tool availability is controlled by profile-aware descriptors
3. `ToolRegistry` still aggregates built-ins and ACP tools in one place
4. name-based lookup semantics remain unchanged and explicitly validated
