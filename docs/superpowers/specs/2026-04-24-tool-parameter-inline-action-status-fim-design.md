# Tool Parameter Inline Layout, Action Status, and FIM Repair Design

## Problem Statement

Current interaction has three UX/behavior gaps:

1. Tool parameters are shown under the tool title instead of inline at the right side of the tool name.
2. Streaming indicators are shown in multiple places (including lower-left), creating noisy and duplicated status signals.
3. Tool-argument JSON errors rely on static repair rules only; there is no focused generation path (e.g. FIM-style constrained rewrite) when errors persist.

Requested direction:

1. Render tool parameters to the right of tool name.
2. Remove all standalone streaming indicators, especially lower-left.
3. Replace generic streaming label with explicit current action status (e.g. thinking / planning / executing).
4. Introduce a FIM-style strategy for correcting malformed tool arguments.

## Goals

1. Make tool rows compact and scannable by placing arguments inline with tool title.
2. Reduce status noise by removing duplicate streaming surfaces.
3. Expose one clear “current action” status model in users/status area.
4. Improve tool-argument repair success for repeated malformed payloads without broad unsafe auto-fix.

## Non-Goals

1. Redesigning overall timeline structure.
2. Replacing existing parser/repair safeguards with unconstrained free-form generation.
3. Introducing FIM for general assistant responses or code editing in this scope.

## Approaches

### Approach A: Pure UI Patch + Existing Repair Rules

- Move parameter rendering inline.
- Remove streaming widgets and rename status label.
- Keep current repair pipeline unchanged.

Trade-off:

- Fastest UI cleanup.
- Leaves recurring malformed-args failures mostly unchanged.

### Approach B: UI Patch + Targeted FIM Repair Fallback (Recommended)

- UI cleanup from Approach A.
- Add bounded FIM-style fallback only for tool-args correction after normal repair failure.

Trade-off:

- Slightly more runtime complexity.
- Best balance of UX and robustness.

### Approach C: Full Generation-based Repair First

- Use model rewrite for most malformed payloads before deterministic repair.

Trade-off:

- Higher recovery chance in some cases.
- Higher unpredictability and safety risk; weak fit with strict-default runtime policy.

## Chosen Approach

Adopt **Approach B**:

1. UI side: inline parameter layout + remove streaming widgets + action-first status text.
2. Runtime side: preserve deterministic repairs first; add constrained FIM-style correction only as final bounded fallback for tool arguments.

## Design

### 1. Tool Row Layout: Parameter Inline Right

Update tool descriptor rendering so each tool row header is one line:

1. Left: status marker + tool name (+ badge).
2. Right: compact parameter summary.

Rules:

1. Parameter summary truncates safely by display width with ellipsis.
2. Full parameter payload remains in details expansion.
3. Running/completed/error styles stay unchanged.

Affected area:

- `src/chat/view/timeline/render_entry/tooling/{descriptor.rs,renderer.rs}`

### 2. Remove Standalone Streaming Surfaces

Remove all dedicated streaming widget rendering:

1. Delete timeline-zone streaming row allocation.
2. Stop rendering `render_streaming_indicator` in frame.
3. Keep state-driven status in users region only.

Affected area:

- `src/chat/view/mod.rs`
- `src/chat/view/streaming.rs` (retire or keep as dead module removal task)

### 3. Action-First Status Model

Replace generic “streaming…” copy with explicit action labels:

1. `thinking`
2. `planning`
3. `executing <tool>`
4. `ready`
5. `error: ...`

State source:

1. Primary from `TurnKind` + runtime turn state.
2. Optional tool-aware status from active tool call metadata in app event handling.

Design rule:

- One authoritative status surface in users/status row; no duplicate lower-left progress text.

Affected area:

- `src/chat/app.rs` (`status_label` and related active-turn fields)
- `src/chat/app/events.rs` (tool execution status transitions)
- `src/chat/view/status.rs` (status row rendering)

### 4. FIM-style Tool-Arg Correction Strategy

Scope:

1. Only for tool arguments.
2. Trigger only after deterministic parser repair path fails.

Flow:

1. Existing parse/repair stages run first (`extract`, control chars, targeted bash fixes, etc.).
2. On repeated parse failure for same signature, construct a constrained correction prompt:
   - Prefix: stable context (`tool_name`, schema expectation, error).
   - Middle: malformed argument payload.
   - Suffix: strict JSON shape requirement.
3. Ask model for corrected JSON payload only.
4. Validate with strict parse + schema/shape checks before execution.
5. If still invalid, return actionable error to model (existing failure loop).

Safety constraints:

1. Retry budget: max 1 FIM correction attempt per failed call signature.
2. No execution if corrected payload fails strict validation.
3. Emit structured event for any FIM correction attempt/result.

Affected area:

- `src/agent/runtime/scheduler.rs`
- `src/agent/runtime/json_repair.rs`
- provider/runtime integration seam for correction request (new helper only, no full protocol rewrite)

### 5. Error Handling and Observability

Add/extend structured events:

1. `tool_args_fim_correction_requested`
2. `tool_args_fim_correction_succeeded`
3. `tool_args_fim_correction_failed`

Include:

1. tool name
2. call id
3. parse error category
4. repaired flag

### 6. Test Strategy

Add tests in three groups:

1. **UI rendering**
   - Tool row shows parameter summary inline to the right.
   - No streaming row rendered in frame.
   - users/status row shows action-first labels.
2. **State transitions**
   - status label switches `thinking/planning/executing/ready`.
3. **FIM correction**
   - deterministic repairs still preferred.
   - FIM fallback invoked only after deterministic failure.
   - corrected payload must parse; invalid correction remains error.
   - single-attempt budget enforced.

## Risks and Mitigations

1. **Risk**: Inline parameter text overflows and hurts readability.
   - **Mitigation**: width-aware truncation and detail expansion.
2. **Risk**: FIM fallback introduces non-deterministic behavior.
   - **Mitigation**: strict gating, single retry budget, parse/schema validation.
3. **Risk**: Removing streaming widget loses perceived responsiveness.
   - **Mitigation**: richer action label updates in status row and tool rows.

## Review Checklist (Self-Validated)

1. No placeholders/TBDs.
2. Requirements covered: inline params, streaming removal, action status, FIM repair strategy.
3. Scope focused to this feature set, no unrelated architecture rewrite.
4. Safety boundaries for FIM behavior are explicit and testable.
