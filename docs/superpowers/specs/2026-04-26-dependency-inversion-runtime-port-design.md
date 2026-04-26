# Dependency Inversion for Chat Runtime Boundary

## Problem

`ChatApp` currently depends on concrete runtime implementation details (`AgentRuntime`), which increases coupling and makes boundary testing/refactoring harder.

Primary coupling points:

- `ChatApp` field holds concrete `AgentRuntime`
- message dispatch path directly calls runtime concrete APIs
- model switch / ACP refresh / cancel / event drain are bound to concrete type

Goal: make upper-layer chat workflow depend on an abstraction (trait) rather than concrete runtime type, while preserving current behavior.

## Scope

### In scope (Phase 1)

1. Introduce a runtime boundary trait for Chat layer (`AgentRuntimePort`)
2. Make `ChatApp` depend on `Box<dyn AgentRuntimePort>`
3. Implement adapter via existing `AgentRuntime`
4. Preserve all existing event/state semantics and status label behavior
5. Keep command/runtime behavior unchanged from user perspective

### Out of scope (Phase 1)

1. Reworking provider/runtime internal architecture
2. Refactoring tool scheduler/executor abstractions
3. Full command-layer abstraction (`CommandHost`) beyond minimal compatibility
4. Any UI behavior changes

## Options considered

### A) ChatApp -> Runtime trait inversion only (chosen)

Introduce a single trait boundary at Chat layer and keep runtime internals unchanged.

Pros:

- smallest safe change surface
- aligns with current state ownership (`ChatApp` keeps product state)
- easy to validate with current tests

Cons:

- coupling inside command context still exists
- deeper runtime internals remain concrete

### B) A + command-layer host trait

On top of A, abstract command orchestration from concrete `ChatApp`.

Pros:

- improves modularity for palette command subsystem

Cons:

- broader API design changes in command context
- higher migration complexity than needed for first cut

### C) End-to-end abstraction (chat/commands/runtime/provider)

Refactor all major layers around traits in one pass.

Pros:

- strongest inversion end-state

Cons:

- high regression risk
- difficult to isolate breakage and validate incrementally

## Chosen design (A)

### 1. Runtime boundary trait

Add a Chat-facing runtime abstraction:

```rust
pub trait AgentRuntimePort: Send {
    fn submit_turn(&self, prompt: &str) -> Result<(), String>;
    fn set_model(&self, model: &str) -> Result<(), String>;
    fn refresh_acp_agents(&self) -> Result<(), String>;
    fn cancel(&self);
    fn try_recv(&self) -> Option<crate::agent::protocol::AgentEvent>;
}
```

`AgentRuntime` implements this trait directly (or through a thin adapter type).

### 2. ChatApp dependency inversion

Replace concrete field:

- from: `agent_runtime: AgentRuntime`
- to: `agent_runtime: Box<dyn AgentRuntimePort>`

All call sites continue to use the same behavior contract:

- submit turn
- cancel in-flight turn
- poll events in `tick()`
- set model
- refresh ACP agents

### 3. Construction strategy

Keep production construction flow unchanged:

- `ChatApp::new()` still calls provider loader
- still builds runtime with the same workspace/model inputs
- then boxes it as trait object

Keep test path stable:

- `new_for_test()` still uses Echo provider runtime
- same event semantics; only storage type changes

### 4. Ownership and state constraints

No product state moves out of `ChatApp`:

- pending queue state
- status mode / user mode
- active turn metadata
- timeline mutation

Trait only abstracts execution boundary, not application state.

## Data flow after inversion

1. `ChatApp` dispatches prompt through `AgentRuntimePort::submit_turn`
2. Runtime emits `AgentEvent` stream
3. `ChatApp::tick()` drains `try_recv()`
4. `apply_agent_event()` mutates Chat-owned product state
5. rendering layer continues to read from `ChatApp` state only

## Error handling

No silent fallback changes:

- submit/model/refresh failures remain surfaced as warnings/hints in timeline
- cancellation behavior unchanged
- runtime protocol errors continue through existing `TurnFailed` path

## Testing strategy

1. Existing unit tests around:
   - queue admission and dispatch
   - status label transitions
   - event handlers
   should remain green without semantic rewrites
2. Add focused tests for trait boundary wiring:
   - `ChatApp` can work with a fake runtime implementing `AgentRuntimePort`
   - dispatch and poll behavior remains deterministic
3. Regression check:
   - verify no changes to timeline/status outputs for existing fixtures

## Rollout plan

1. Introduce trait and impl
2. Switch `ChatApp` field/type signatures
3. Adapt constructors
4. Add boundary-focused tests
5. Run full fmt/lint/test gates

## Risks and mitigations

1. **Trait-object lifetime/object safety issues**
   - keep trait minimal and object-safe (`&self` methods, concrete return types)
2. **Hidden behavior changes during constructor migration**
   - preserve existing runtime construction sequence exactly
3. **Test brittleness from type-level changes**
   - keep external behavior assertions unchanged; only wiring tests are added

## Success criteria

1. `ChatApp` no longer references concrete `AgentRuntime` in its field type
2. Core chat behavior remains unchanged under existing test suite
3. New boundary tests validate upper-layer dependency on trait abstraction

