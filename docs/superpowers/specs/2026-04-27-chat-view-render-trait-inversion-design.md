# Chat View Render Trait Inversion Design

## 1. Problem Statement

Current chat rendering has improved local modularity, but top-level view orchestration still contains direct knowledge of concrete render functions and module-specific wiring. This weakens dependency inversion and makes new render surfaces expensive to add safely.

We need a uniform render contract so upper layers depend on abstractions only, while concrete rendering logic stays inside component modules.

## 2. Goals and Scope

### Goals

1. Make `view` orchestration depend on trait contracts, not concrete renderers.
2. Keep product state ownership in `ChatApp` and other Blazar-owned state types.
3. Make render components pluggable: add/replace via implementation + registration.
4. Preserve existing product semantics, while allowing minor layout/interaction improvements that do not alter core behavior.

### Out of Scope

1. Replacing Blazar state model with widget-owned state.
2. Introducing runtime plugin loading or dynamic external renderer discovery.
3. Rewriting unrelated runtime/tooling architecture.

## 3. Proposed Architecture

### 3.1 Layering

Three explicit layers:

1. **Orchestrator layer** (`src/chat/view/mod.rs`)
   - Computes layout slots.
   - Builds render context.
   - Dispatches render calls to abstract registry interface.
   - Retains orchestration-local layout helpers and frame-level behavior (for example streaming indicators and picker visibility checks), while delegating slot rendering through contracts.
2. **Contract layer** (new, e.g. `src/chat/view/render/contracts.rs`)
   - Defines shared trait(s), context, slot identity, and error types.
   - Defines registry abstraction used by orchestrator.
3. **Component layer** (`timeline/users/input/status/picker` modules)
   - Implements render trait for each component.
   - Owns concrete rendering details and local composition.

This keeps orchestration stable while component logic evolves independently.

### 3.2 Core Contracts

Define a common contract similar to:

- `RenderUnit`: render one surface slot with `(frame, slot_area, context)`.
- `RenderCtx`: read-only projection of render-time state (`app`, `theme`, layout policy, tick).
- `RenderSlot`/`RenderKind`: enum-like identity for top-level slots.
- `RenderRegistry`: abstraction that resolves `RenderSlot` to `RenderUnit`.
- `RenderError`: typed failures (`RegistryMissingSlot`, `ContextMismatch`, `ComponentError`).

Upper-layer dispatch code (`render_frame*`) references `RenderRegistry` + contracts for slot rendering. Layout planning and other orchestration-only frame concerns may remain local to `view/mod.rs`.

## 4. Data Flow

1. Orchestrator computes layout areas for the timeline, users sub-slots (top/input/status plus separators), and picker overlay.
2. Orchestrator creates one `RenderCtx` from `ChatApp` and frame-level inputs.
3. For each slot, orchestrator dispatches via `RenderRegistry`.
4. Each component renders internally; complex components may sub-dispatch locally, but still present one top-level contract to orchestration.

Adding a new top-level render surface becomes:

1. Add slot identity.
2. Implement `RenderUnit`.
3. Register in assembly.

No direct orchestrator edits beyond layout declaration are required.

## 5. Error Handling and Observability

1. `RenderUnit::render` returns `Result<(), RenderError>`.
2. Orchestrator handles errors centrally:
   - emits structured diagnostics/logging,
   - degrades only the affected region with a minimal error row,
   - preserves whole-frame render stability.
3. Missing slot registrations must be explicit errors (never silent no-op).

This prevents one renderer failure from breaking the entire TUI.

## 6. Testing Strategy

### 6.1 Contract Tests

Verify all render units satisfy contract behavior for:

- zero-area slot,
- narrow-width truncation constraints,
- mode-dependent rendering branches.

### 6.2 Registry/Assembly Tests

Verify each required top-level slot is bound in the default registry, and missing bindings surface explicit errors.

### 6.3 Regression Tests

Keep existing render snapshots and interaction tests as semantic guardrails; refactor must preserve intended behavior.

### 6.4 Incremental Migration

Migrate in bounded phases:

1. Introduce contracts + registry abstraction.
2. Migrate timeline/users.
3. Migrate input/status/picker.
4. Remove legacy direct render wiring.

After each phase run repository quality gates.

## 7. Safety and State Ownership Constraints

1. Product state remains in Blazar-owned state (`ChatApp` et al.).
2. Components are pure render consumers of read-only context, not hidden state owners.
3. UI improvements are allowed only if they do not alter core product semantics.

## 8. Acceptance Criteria

1. `view/mod.rs` routes slot rendering through render contracts and the registry abstraction, without direct concrete renderer calls in orchestrator dispatch.
2. Required slot coverage includes the timeline, users top/input/status sub-slots, separator rows, and picker overlay.
3. Missing slot registrations surface explicit typed errors instead of silent no-ops.
4. Existing semantics remain intact aside from explicitly accepted minor UI improvements, with regression coverage kept green.
5. Repository quality gates remain green.

## 9. Risks and Mitigations

1. **Risk:** abstraction overhead and fragmented logic.
   - **Mitigation:** keep one clear top-level contract and minimal trait surface.
2. **Risk:** missing renderer registration during migration.
   - **Mitigation:** explicit registry tests + typed missing-slot error.
3. **Risk:** behavior drift hidden by refactor noise.
   - **Mitigation:** retain snapshot/interaction regression tests and migrate incrementally.
