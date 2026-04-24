# Plan Step Status Label Design

## Problem

For the **thinking state specifically**, users want an **AI-named short action** (for example `explore`)
instead of the generic `thinking` label.

## Goal

Show a concise action-like label only during thinking, sourced from a name explicitly provided by the model during output.
The name must stay short.

## Non-Goals

1. Redesigning timeline layout.
2. Changing tool execution behavior.
3. Introducing a new panel.

## Approaches

### A) Latest pending plan step (recommended)

- Parse the latest plan response into step items.
- Use the first pending step label as the status label.
- Fallback when no plan step is available.

**Pros:** simple, stable, directly matches user intent (`explore`-like labels).  
**Cons:** does not track true execution progress unless step completion is inferred.

### B) Explicit plan cursor tracking

- Keep a current plan-step cursor in app state and move it with events.

**Pros:** most accurate progress semantics.  
**Cons:** more state complexity and harder to keep robust for all turn flows.

### C) Plan title only

- Show plan title text only.

**Pros:** minimal change.  
**Cons:** does not provide step-level labels.

## Chosen Design

Use **Approach A** in **thinking-only scope**.

1. Extract normalized step labels from plan responses.
2. Store the latest plan steps in `ChatApp` state.
3. `status_label()` returns the short step name **only when current label would be `thinking`**.
4. Keep `executing <tool>`, `planning`, `ready`, and `error: ...` behavior unchanged.

## Architecture and Data Flow

1. **Output parsing layer** (`chat/app/events.rs`):
   - Parse a dedicated naming line from thinking/output text.
   - Protocol (v1): `next_step_name: <short-name>`.
   - Reuse conservative fallback extraction when protocol line is absent.

2. **State layer** (`chat/app.rs`):
   - Add field for latest parsed plan step labels.
   - Add helper to compute status label from plan steps.

3. **Render layer** (`chat/view/status.rs`):
   - Continue rendering `app.status_label()` as single authoritative status surface.
   - No additional panel or row.

## Parsing Rules (v1)

1. Preferred protocol line in output: `next_step_name: explore`.
2. Normalize to short lower-case label and trim whitespace.
3. Keep name short: max 12 visible characters (truncate with ellipsis when needed).
4. If protocol line is absent, fallback to conservative first-action extraction from text.
5. If parsing fails, preserve safe fallback behavior (`thinking` stays as-is).

## Error Handling

1. Never panic on malformed naming lines.
2. If no valid name extracted, status falls back safely to `thinking`.
3. Do not silently alter non-thinking turn behavior beyond status text selection.

## Testing

1. Unit tests for step extraction:
   - numbered/bulleted/checklist formats
   - mixed markdown noise
   - unicode labels
2. `ChatApp` tests for status priority:
   - thinking state + parsed step available → short step label shown
   - thinking state + no parsed step → `thinking`
   - non-thinking states remain unchanged (`planning`, `executing <tool>`, `ready`, `error: ...`)
3. Status row rendering test:
   - step label appears in right status segment without layout regressions.

## Risks and Mitigations

1. **Risk:** over-aggressive parsing picks wrong words.  
   **Mitigation:** conservative pattern matching + tests.

2. **Risk:** stale step labels persist too long.  
   **Mitigation:** clear/refresh step cache on new non-plan turn boundaries.

## Scope Check

This is a single focused subsystem change (status label source). No decomposition needed.

## Self-Review

1. No TODO/TBD placeholders.
2. Approach and chosen design are consistent.
3. Scope is limited to requested behavior change.
4. Ambiguity reduced via explicit parsing and fallback rules.
