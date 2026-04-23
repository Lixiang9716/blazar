# Agent-as-Tool Boundary Refactor (Capability Kernel) Design

## Problem

The current Agent-as-Tool implementation works, but protocol concerns, scheduling concerns, and tool identity concerns are still spread across multiple layers. This creates coupling between runtime, tool implementations, and protocol-specific behavior, which makes large refactors risky.

For the next refactor phase, we need a structure that optimizes for:

- clear module boundaries
- parallel tool-call safety
- protocol pluggability (with Agent Client Protocol as the primary protocol)

## Design Goals

1. Keep Blazar-owned product state in Blazar state types.
2. Make protocol adapters replaceable without changing runtime orchestration.
3. Centralize concurrency and resource-conflict rules.
4. Preserve behavior with staged migration (no big-bang rewrite).

## Chosen Approach

Adopt a **Capability Kernel** architecture:

- `Tool` remains the facade seen by existing runtime/provider flow.
- A new capability layer becomes the execution core.
- ACP SDK integration is isolated in a dedicated adapter module.
- Scheduler and result projection rules are centralized and protocol-agnostic.

This balances structure and migration safety better than:

- a pure Tool-first extension (too much long-term coupling), or
- a protocol-bus-first rewrite (too risky for current stage).

## Target Architecture

### 1. Facade Layer (stable entrypoints)

- `ToolRegistry`
- `AgentRuntime`

This layer keeps existing external behavior and routes execution to capabilities.

### 2. Capability Kernel (new execution core)

Defines shared execution contracts:

- `CapabilityHandle`
- `CapabilityInput`
- `CapabilityResult`
- `CapabilityError`
- `ResourceClaim`
- `ConflictPolicy`

Both local tools and ACP-backed agent tools implement capability semantics through this layer.

### 3. Protocol Adapter Layer

- `AcpClientAdapter` (primary protocol adapter)
- SDK dependency boundary for `agentclientprotocol/rust-sdk`
- ACP event/status/payload mapping into kernel types

No runtime state ownership belongs here.

### 4. Infrastructure Layer

- filesystem and path safety helpers
- shell execution primitives
- config/discovery loading
- logging and diagnostics

## File Layout (Phased Structural Migration)

### Phase 1 layout target

- `src/agent/runtime/`
  - `executor.rs`
  - `scheduler.rs`
  - `events.rs`
- `src/agent/capability/`
  - `mod.rs`
  - `local/`
  - `acp/`
- `src/agent/adapters/acp_client/`
  - `client.rs`
  - `mapper.rs`
- `src/agent/tools/` (kept as facade during migration)
- `src/agent/state.rs` (remains Blazar-owned state source of truth)

### Migration rule

Each step must remain buildable and testable. Introduce forwarding layers first, then move implementations behind them.

## Data Flow and Parallel Safety

Unified execution flow:

`Provider tool_calls -> CapabilityPlanner -> BatchScheduler -> CapabilityExecutor -> ResultProjector -> ProviderMessage::ToolResult`

### Scheduling rules

- same-resource `ReadOnly` + `ReadOnly`: parallel
- any same-resource `ReadWrite`: serialized
- `Exclusive`: globally conflicting

### Resource key normalization

All claims go through a canonicalizer so path aliases collapse to one scheduler identity (`src/a.rs` and `./src/a.rs` are identical claims).

### Ordering guarantees

- batches execute serially
- calls inside a batch run in parallel
- completion replay is emitted in original call order for deterministic UI and tests

## Protocol Boundary Rules (ACP-first)

1. ACP adapter is the only layer aware of ACP SDK event types.
2. Runtime depends on kernel-level abstractions, not ACP SDK types.
3. Protocol payload details must be projected into `CapabilityResult` / `CapabilityError`.
4. ACP and local capabilities share the same scheduler and lifecycle semantics.

## Error Handling

- Failed/cancelled ACP terminal states must always map to error outcomes.
- Discovery is degradable:
  - configured collisions/errors: explicit failure or warning based on policy
  - discovered collisions: warning + skip
- No silent drops for meaningful outputs.

## Result Projection Rules

`CapabilityResult` supports:

- plain text output
- resource outputs (URI + MIME metadata)
- structured error metadata

Downstream text paths must preserve resource summaries (not drop them), so non-text ACP outputs remain actionable.

## Testing Strategy

### Unit

- claim normalization
- conflict matrix behavior
- ACP status mapping and terminal-state error semantics
- result projection (text + resource)

### Integration

- config-first then discovery-second registration
- built-in name collision policy
- partial discovery failure behavior

### Runtime/E2E

- deterministic ordering under parallel batches
- cancellation semantics
- timeline rendering for ACP-vs-local identity

## Phased Delivery Plan (architecture-level)

1. Introduce capability kernel contracts.
2. Move ACP execution behind adapter boundary.
3. Move local tools behind capability/local.
4. Make runtime depend only on capability contracts.
5. Remove temporary forwarding/compatibility scaffolding.

## Non-Goals

- No big-bang UI rewrite in this sub-project.
- No replacement of Blazar-owned state with widget/adaptor-owned state.
- No protocol-generalization that delays ACP-first deliverability.

