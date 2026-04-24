# Queued User Pending Indicator Design

## Problem Statement

Blazar currently blocks execution of user input while the assistant is streaming and queues the input for the next reaction cycle. This prevents assistant output from being split into multiple timeline entries, but queued user input is invisible until dispatch.

Target behavior:

1. While assistant is streaming, submitted user input is blocked and queued.
2. Queued user input is visible immediately with a pending marker (`xxx (pending)`).
3. The actual input is only dispatched in the next turn cycle.
4. Assistant streaming continuity is preserved (no split caused by pending visualization).

## Assumptions

- User was unavailable during clarification; default pending display style is chosen:
  - show pending text as user-style timeline rows with suffix `(pending)`.

## Goals

- Preserve existing queue semantics and safety.
- Make queued user intent visible without breaking streaming continuity.
- Keep rendering consistent with existing timeline style for user messages.

## Non-Goals

- Rework provider streaming protocol.
- Change command palette semantics.
- Introduce multi-turn parallel execution.

## Approaches Considered

### Approach A (Recommended): Renderer-derived pending rows from queue state

- Keep queued inputs in `pending_messages` as source of truth.
- Do **not** append pending items to `timeline` data.
- In timeline rendering, append derived user-style rows for queued inputs with `(pending)` suffix.
- On dispatch, pending row disappears automatically because queue is popped; dispatched user message is materialized normally.

Pros:
- Preserves assistant streaming continuity with minimal runtime risk.
- Reuses existing queue state, avoids duplicate state ownership.
- Small, localized change in rendering and tests.

Cons:
- Pending rows are view-derived, not persisted timeline entries.

### Approach B: Real timeline pending entries + active assistant target index

- Insert actual pending entries into `timeline`.
- Update text delta appends to target active assistant entry instead of `timeline.last()`.

Pros:
- Pending is fully represented as timeline data.

Cons:
- Higher risk and broader runtime changes.
- More complex invariants.

### Approach C: Status-strip-only pending count

- Show only summary like `2 queued` in status bar.

Pros:
- Minimal implementation.

Cons:
- Fails requirement to show concrete `xxx pending` content in timeline style.

## Selected Design

Use **Approach A**.

## Architecture and Components

1. **Queue state remains authoritative**
   - `pending_messages` in `ChatApp` remains the only source of queued input.

2. **Read-only accessor for rendering**
   - Add a narrow accessor from `ChatApp` to expose queued user texts for view rendering.
   - No mutation in view layer.

3. **Timeline renderer augmentation**
   - After rendering regular timeline entries, append derived rows for each queued item:
     - same user marker/prefix style as user message rows
     - text body: `<user_text> (pending)`
   - These rows are non-persistent visualization only.

4. **Dispatch behavior unchanged**
   - Existing dispatch-time materialization remains:
     - queued item popped
     - user message materialized in messages/timeline
     - runtime/command dispatch proceeds

## Data Flow

1. User submits during streaming.
2. `send_message` queues pending turn and returns.
3. Next render pass reads `pending_messages` and shows derived `xxx (pending)` row.
4. Current assistant stream continues appending to assistant entry (unchanged continuity).
5. On `TurnComplete`/failure continuation, queue dispatch pops item.
6. Pending row disappears; real user entry appears when dispatch materializes it.

## Error Handling

- Keep current queue progression behavior on:
  - runtime synchronous dispatch failure
  - discover enqueue failure
  - discover refresh success/failure events
- Pending visualization must track queue state only; no separate cleanup path required.

## Testing Plan

Add/extend tests in `tests/unit/chat/app/tests_impl.inc` and timeline render tests:

1. **Queued pending visualization**
   - busy submit queues message
   - rendered timeline includes `xxx (pending)` line
   - underlying timeline entries remain unchanged until dispatch

2. **Pending removed on dispatch**
   - after dispatch trigger, pending marker for that item no longer rendered
   - real user message appears in normal timeline flow

3. **Streaming continuity preserved**
   - assistant `TextDelta("A")`, queue input, `TextDelta("B")`
   - assistant response remains contiguous (`AB`) while pending row exists as derived view output

4. **Command queue continuation compatibility**
   - queued `/discover-agents` success/failure still progresses queue; pending markers update accordingly

## Acceptance Criteria

- During streaming, queued input is visible as `xxx (pending)` in user-style timeline rendering.
- Pending visualization never splits current assistant streaming message entry.
- Pending marker disappears when item is dequeued for dispatch.
- Existing queue continuation and failure semantics remain intact.
