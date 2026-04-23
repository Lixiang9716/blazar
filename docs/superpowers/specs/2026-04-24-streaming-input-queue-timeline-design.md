# Streaming Input Queue and Timeline Integrity Design

## Problem Statement

Blazar currently allows the user to submit new input while an assistant turn is streaming. The input is queued for execution, but the user timeline entry is inserted immediately. This can split an in-progress assistant response into multiple `TimelineEntry::response` blocks because `TextDelta` appends to the last assistant message entry only when it is still the last timeline entry.

Required behavior:

1. While assistant output is streaming, user commands/messages are blocked from immediate execution.
2. Inputs are accepted and queued.
3. Queued user inputs are only inserted into timeline when they are actually dispatched in the next reaction cycle.
4. Assistant streaming output remains in one time entry for that turn unless intentionally segmented by assistant-side events.

## Goals

- Preserve FIFO input queue semantics.
- Prevent user-side timeline insertion from fragmenting assistant output.
- Keep existing turn lifecycle (`TurnComplete`/`TurnFailed` -> dispatch next queued) as the dispatch trigger.
- Apply consistent blocking semantics to both runtime turns and local message commands submitted through the composer.

## Non-Goals

- Reworking provider/tool streaming behavior.
- Redesigning timeline rendering.
- Introducing parallel user turn execution.

## Chosen Approach

Use delayed timeline insertion (Approach A) with queue-owned dispatch responsibility.

Core decision:

- Any user submission while busy is queued only.
- No user timeline entry is inserted at queue time.
- Timeline insertion occurs exactly once when the queued item is dispatched.

## Design

### 1. Pending Queue Model

Represent queue items as dispatchable actions, not only runtime prompts.

- `RuntimeTurn { user_text, runtime_prompt, kind, timeline_inserted }`
- `LocalCommand { user_text, command, timeline_inserted }` (for submit-path local commands like `/discover-agents`)

`timeline_inserted` starts as `false` and is set to `true` when dispatch performs insertion.

This guarantees one-time insertion and avoids duplicate user entries across retries or helper-path reuse.

### 2. Submission Flow (`send_message`)

1. Normalize input and build a pending action.
2. If busy:
   - push to `pending_messages`
   - return without timeline mutation and without command execution
3. If idle:
   - dispatch immediately via the same dispatch helper used by queued items

Important invariant:

- `send_message` does not directly mutate timeline for user message entries.
- Dispatch is the single place that inserts user entries and triggers execution.

### 3. Dispatch Flow (`dispatch_next_queued` + immediate dispatch helper)

Dispatch helper behavior:

1. If `timeline_inserted == false`, append `TimelineEntry::user_message(user_text)` and mark inserted.
2. Execute action:
   - `RuntimeTurn` -> set `active_turn_kind/title`, call `submit_turn`
   - `LocalCommand` -> execute local behavior (e.g. ACP refresh)
3. On execution failure, append warning entry and continue queue progression rules.

`TurnComplete` and `TurnFailed` keep calling `dispatch_next_queued()`, preserving current scheduling semantics.

### 4. Timeline Integrity Invariant

During streaming, user submissions do not create timeline entries. Therefore, intermediate `TextDelta` events continue appending to the same assistant response entry, so assistant output is not split by user input insertion.

Expected ordering for queued submissions:

- assistant (current turn, streaming and completion)
- next queued user entry appears
- next assistant turn starts

### 5. Error Handling

- Runtime submission failure:
  - append warning entry (`Runtime error: ...`)
  - clear active turn fields for failed dispatch
  - continue dispatch loop behavior without silent drops
- Local command enqueue/dispatch failure:
  - append warning entry
  - do not execute command early while busy
- No broad catch-all suppression; failures remain visible in timeline.

## Testing Plan

Update/add unit tests in `tests/unit/chat/app/tests_impl.inc`:

1. Update `send_message_queues_when_agent_busy`:
   - queue length assertions remain
   - timeline must not contain newly queued user entries before dispatch
2. Add `queued_message_inserts_user_entry_on_dispatch`:
   - busy submit queues only
   - post-`TurnComplete` dispatch inserts user entry
3. Add `streaming_text_not_split_by_queued_user_input`:
   - `TextDelta("A")`, queue user input while busy, `TextDelta("B")`
   - assert single assistant response entry contains `AB`
4. If local command queueing is implemented:
   - add `discover_agents_queued_while_busy_and_runs_after_turn`
   - assert no immediate hint during busy, then command executes on dispatch

## Review Checklist (Self-Validated)

- No unresolved placeholders.
- No contradiction between queue semantics and timeline semantics.
- Scope is focused to chat submission/dispatch/timeline integrity.
- Behavioral expectations are explicit for busy vs idle paths.
