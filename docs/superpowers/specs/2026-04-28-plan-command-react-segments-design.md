# `/plan` Plugin: Multi-Segment ReAct Design

## Problem

Current `/plan` only prepares input text (`/plan `) and does not own a full planning-and-execution workflow.  
Target behavior is a plugin-driven `/plan` flow where planning and execution are decomposed into multiple ReAct segments.

## Goals

1. Keep `/plan` as a plugin command.
2. Decompose `/plan` workflow into multiple ReAct segments.
3. Use strict micro-steps: one minimal decision per ReAct cycle.
4. Persist plans for team sharing in Git.
5. Support fast local querying via SQLite index.
6. Avoid plan-specific coupling in core chat modules.

## Non-Goals

1. No Reflexion memory layer.
2. No new global mode command surface.
3. No rewrite of core turn/event semantics.

## High-Level Approach

`/plan` is implemented as a plugin-owned workflow with a state machine and storage layer:

- `command` handles plugin entry and command-level routing.
- `session` owns segment state machine and transitions.
- `store` owns JSON persistence and SQLite indexing.

Core remains generic; only minimal generic extension points may be added in command infrastructure when needed.

## Segment State Machine (All ReAct Segments)

Phases:

1. `Discover`
2. `Clarify`
3. `DraftStep`
4. `FinalizePlan`
5. `ExecuteStep`
6. `Review`
7. `Done`

Rules:

1. One ReAct cycle advances one minimal decision only.
2. A phase may require multiple ReAct cycles before exit.
3. After each execution step, always return to ReAct for next-step decision.
4. Failures are handled within current plan session (retry/revise/abort), without cross-session memory.

## Plugin Folder Structure

`src/chat/commands/builtins/plan/`

1. `command.rs`
   - `/plan` plugin entry
   - argument parsing and dispatch
2. `session.rs`
   - `PlanSession` model
   - phase transition engine
   - phase exit criteria
3. `store.rs`
   - JSON read/write
   - SQLite index sync/rebuild

`src/chat/commands/builtins/plan.rs` acts as module entry and re-exports.

## Persistence Model (Hybrid)

### Git Source of Truth

`.blazar/plans/<plan_id>.json`

Recommended fields:

- `id`
- `created_at`
- `updated_at`
- `status` (`pending`, `executing`, `completed`, `failed`, `cancelled`)
- `goal`
- `phase`
- `steps` (ordered with per-step status)
- `current_step`
- `events` (decision/status summary only)

### Local Query Index (Not in Git)

`.blazar/state/plan_index.db`

Tables:

1. `plans`
2. `plan_steps`
3. `plan_events`

Sync rules:

1. Write JSON first.
2. Update SQLite index second.
3. On startup/index mismatch, rebuild index from JSON.

## Execution Interaction

1. User runs `/plan <goal>`.
2. Session enters `Discover` and progresses segment-by-segment.
3. In `Clarify`, plugin may issue `ask_user` and loop until required inputs are complete.
4. In `FinalizePlan`, plugin emits final plan summary.
5. Plan execution proceeds as segmented `ExecuteStep` + `Review` cycles until `Done`.

## Failure Handling

1. Step/action failure: transition to `Review`, decide `retry` or `revise`.
2. Missing required information: transition to `Clarify`.
3. Unrecoverable condition: transition to `Done(failed)`.
4. User cancellation: transition to `Done(cancelled)`.

## Testing Strategy

### Unit Tests (`plan/`)

1. `session`: phase transitions, micro-step behavior, loop exits.
2. `store`: JSON schema write/read, SQLite sync, rebuild correctness.
3. `command`: routing, `/plan` startup behavior, clarify path triggering.

### Integration Tests

1. End-to-end happy path:
   `Discover -> Clarify -> DraftStep -> FinalizePlan -> ExecuteStep -> Review -> Done`
2. Failure path:
   execution failure -> revise/retry -> success or `Done(failed)`.
3. Persistence path:
   restart/reload retains JSON truth and rebuilds index when needed.

## Rollout

1. Land plugin folder structure and state machine first.
2. Land storage layer second.
3. Enable segmented execution flow third.
4. Harden with integration tests before defaulting to new behavior.
