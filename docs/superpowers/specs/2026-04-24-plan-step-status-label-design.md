# Plan Step Status Label Design

## Problem

Users do not want status labels like `planning` / `executing`.
They want the users status row to show the **model plan step item** (for example `explore`).

## Goal

Show a concise, action-like label sourced from the latest model plan step, instead of lifecycle words.

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

Use **Approach A**.

1. Extract normalized step labels from plan responses.
2. Store the latest plan steps in `ChatApp` state.
3. `status_label()` returns the first pending/latest relevant step label.
4. If no plan step exists, fallback to `ready` (or existing error label on failed state).

## Architecture and Data Flow

1. **Plan parsing layer** (`chat/app/events.rs` or existing plan-finalization seam):
   - Reuse current plan title/body extraction.
   - Add step-label extraction helper with conservative parsing rules.

2. **State layer** (`chat/app.rs`):
   - Add field for latest parsed plan step labels.
   - Add helper to compute status label from plan steps.

3. **Render layer** (`chat/view/status.rs`):
   - Continue rendering `app.status_label()` as single authoritative status surface.
   - No additional panel or row.

## Parsing Rules (v1)

1. Accept common numbered/bulleted lines:
   - `1. explore repo`
   - `- explore repo`
   - `- [ ] explore repo`
2. Normalize to short lower-case label by taking first action token or short phrase.
3. Keep max display width/truncation behavior consistent with status row rendering.
4. If parsing fails, preserve safe fallback behavior.

## Error Handling

1. Never panic on malformed plan text.
2. If no valid steps extracted, status falls back safely (`ready` or failure label).
3. Do not silently alter non-plan turn behavior beyond status text selection.

## Testing

1. Unit tests for step extraction:
   - numbered/bulleted/checklist formats
   - mixed markdown noise
   - unicode labels
2. `ChatApp` tests for status priority:
   - plan step available → step label shown
   - no plan step + idle/done → `ready`
   - failed turn → `error: ...`
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
