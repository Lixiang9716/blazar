# AI-Friendly Debugging Foundation Design

## Problem Statement

Blazar already has several useful debugging seams:

1. `turn_id` and `call_id` exist in runtime and timeline flows.
2. `TimelineEntry` already stores compact `body` and expandable `details`.
3. `observability::logging` already emits structured JSON lines with event metadata.
4. `ChatApp::apply_agent_event` already bridges runtime events into visible timeline mutations.

However, these seams are still fragmented from an AI-debugging perspective.

Current gaps:

1. Debug evidence is not modeled as a first-class product capability.
2. Structured logs do not consistently preserve enough correlated context to replay a bad turn end-to-end.
3. Timeline details are useful for humans, but not yet organized as an AI-friendly debugging trace.
4. Runtime/task visibility and state-transition visibility are not presented as one coherent debugging story.

Required outcome:

1. A single failed or suspicious turn should be reconstructable from durable evidence.
2. AI should be able to answer: what happened, in what order, under which IDs, and which state changed.
3. Developers should be able to inspect both product-level evidence and async runtime behavior without leaving the system’s mental model.

## Goals

1. Make each turn replayable as a correlated evidence trail.
2. Preserve product-state ownership in Blazar-owned types rather than widget-local state.
3. Expose debugging information through actionable surfaces, not passive decorative panels.
4. Keep the design incremental: phase 1 should grow out of current logging/timeline/runtime seams.
5. Improve debugging for both humans and AI agents working inside Blazar.

## Non-Goals

1. Replace the whole logging stack with full OpenTelemetry in phase 1.
2. Build a generic observability platform or external telemetry backend.
3. Introduce a separate framework-owned debug state model.
4. Re-architect provider/runtime internals beyond what is needed for evidence capture and correlation.

## Chosen Approach

Use an **evidence-first debugging spine**.

Core decision:

1. Treat debugging evidence as a product-layer capability centered on `turn_id`, `call_id`, session/workspace context, and state transitions.
2. Keep one compact event pipeline that can feed:
   - durable debug evidence
   - timeline details and future debug surfaces
   - development-only runtime diagnostics such as `tokio-console`
3. Prioritize replayability and correlation before broad runtime metrics.

Why this approach:

1. It aligns with the existing code structure.
2. It best serves “AI can debug Blazar” rather than “Blazar exports lots of telemetry”.
3. It minimizes architectural risk while unlocking future runtime diagnostics.

## Design

### 1. Debug Evidence Model

Introduce a Blazar-owned debug evidence model that sits above raw logs and below UI surfaces.

Recommended conceptual entities:

1. `DebugTurnTrace`
   - keyed by `turn_id`
   - holds ordered events for one turn
   - stores summary fields such as request text, turn kind, outcome, latest error
2. `DebugEvent`
   - typed event with shared correlation metadata
   - examples: `TurnStarted`, `ThinkingDelta`, `TextDelta`, `ToolCallStarted`, `ToolCallCompleted`, `QueueDispatchStarted`, `QueueDispatchCompleted`, `StateTransition`, `TurnFailed`
3. `StateTransitionEvidence`
   - a compact before/after or old/new summary for meaningful product state changes
   - examples: queue length, active turn kind, agent turn state, active tool count, show-details toggle, current panel/focus

Important constraint:

1. The source of truth remains Blazar product state.
2. Debug evidence is a derived record of state changes and lifecycle events, not a replacement state machine.

### 2. Correlation Schema

Standardize the structured event schema emitted through observability.

Current structured logging already includes:

1. `ts`
2. `level`
3. `target`
4. `event`
5. `message`
6. `trace_id`
7. `turn_id`
8. `tool_name`
9. `agent_id`
10. `error_kind`

Phase 1 should extend this with correlation fields that directly help replay:

1. `call_id`
2. `session_id` or local session key
3. `workspace_path`
4. `turn_kind`
5. `queue_depth`
6. `event_seq` within the turn
7. `state_change` summary payload for transitions that matter

Design rule:

1. Every event that belongs to a turn must carry `turn_id`.
2. Every tool event must carry both `turn_id` and `call_id`.
3. Every error event must identify whether it is provider, protocol, tool, queue-dispatch, or state-transition related.

### 3. Evidence Capture Pipeline

Build one internal pipeline with three layers:

1. **Emitter layer**
   - existing runtime/app code emits typed debug evidence alongside current log/timeline behavior
2. **Recorder layer**
   - collects typed events, assigns per-turn ordering, and persists them
3. **Consumer layer**
   - timeline details
   - future debug pane
   - export/replay helpers

This should be implemented as a Blazar-owned service/module, not spread across UI rendering code.

### 4. Persistence Strategy

Persist debug evidence locally in SQLite alongside other local product data.

Reasoning:

1. The project already uses `rusqlite`.
2. The repo guidance favors SQLite-backed local history and state tracking over unnecessary platform complexity.
3. Durable replayable debugging evidence benefits from queryability and simple local inspection.

Recommended phase-1 tables:

1. `debug_turns`
   - `turn_id`
   - `session_id`
   - `turn_kind`
   - `user_text`
   - `status`
   - `started_at`
   - `completed_at`
   - `last_error_kind`
   - `last_error_message_redacted`
2. `debug_events`
   - `id`
   - `turn_id`
   - `seq`
   - `ts`
   - `event_type`
   - `call_id`
   - `tool_name`
   - `payload_json`
3. `debug_state_transitions`
   - `id`
   - `turn_id`
   - `seq`
   - `state_area`
   - `before_json`
   - `after_json`

Scope guard:

1. Phase 1 does not need a perfect event-sourcing system.
2. It only needs enough normalized evidence to reconstruct one problematic turn accurately.

### 5. UI Surfaces

Phase 1 should add or extend product surfaces that answer actionable questions.

#### 5.1 Timeline debug details

Extend current detail rendering so `Ctrl+O` can expose debug evidence beyond raw tool output.

Examples:

1. turn header metadata
2. tool call correlation IDs
3. queue dispatch markers
4. state transition summaries
5. failure classification with redacted safe logging text

#### 5.2 Dedicated debug pane or inspector

Add one focused debug inspector surface, not a dashboard.

It should answer:

1. What is the active turn?
2. Which tool is currently running or last failed?
3. What changed most recently?
4. Is the queue stalled?
5. Which IDs should an AI follow for this incident?

This pane should remain tightly scoped and action-oriented.

#### 5.3 Replay/export entrypoint

Provide a simple user action to export one turn’s debug bundle or inspect it in-place.

Minimum useful payload:

1. user input
2. ordered events
3. tool args/result summaries
4. state transitions
5. final error or completion status

This is the core AI-debugging affordance: a bundle that can be handed back to the model for diagnosis.

### 6. Runtime Diagnostics

Runtime diagnostics are part of phase 1, but secondary to evidence capture.

Recommended runtime design:

1. Adopt `tracing` / `tracing-subscriber` as the long-term instrumentation surface.
2. Preserve existing structured JSON output expectations during migration.
3. Add a development-only path for `tokio-console`.

Use `tokio-console` for:

1. stuck task analysis
2. tasks that never yield
3. self-wakes / lost-waker style runtime issues
4. queue progression stalls that are actually runtime scheduling problems

Do not make `tokio-console` the primary product debugging story.
It is a developer aid attached to the debugging spine, not the spine itself.

### 7. State Transition Coverage

Not every state mutation should become debug evidence.

Phase-1 capture should focus on high-value state domains:

1. turn lifecycle
2. queued input dispatch
3. active tool set
4. current runtime turn state
5. user-visible warning/hint generation
6. detail expansion and any explicit debug-mode toggles

Design rule:

1. Capture transitions that help explain behavior.
2. Avoid noisy cosmetic-only transitions.

### 8. Error Handling

Error handling should remain explicit and safe.

1. No silent debug-evidence drops without a visible fallback.
2. If evidence persistence fails:
   - keep the product flow alive if possible
   - append a visible warning/hint that debug capture degraded
   - emit a safe fallback log record
3. Sensitive payloads should be redacted or summarized before durable persistence when necessary.
4. Existing “safe log message” patterns for turn failures should be preserved and extended.

### 9. Testing Strategy

Add tests at three levels.

#### 9.1 Unit tests

1. structured event schema completeness
2. event ordering within a turn
3. tool-call correlation (`turn_id` + `call_id`)
4. state transition summarization
5. persistence write/read roundtrip

#### 9.2 Chat/runtime integration tests

1. one streaming turn with multiple tool calls produces a coherent trace
2. failed tool call keeps replay bundle intact
3. queued message dispatch emits queue + turn evidence in correct order
4. timeline details reflect stored evidence consistently

#### 9.3 UI rendering tests

1. timeline detail rendering shows correlation metadata without overwhelming the compact view
2. debug inspector surfaces the most recent error and active IDs
3. debug-only state does not become the source of truth for product state

### 10. Rollout Plan

Phase 1 should be implemented incrementally:

1. standardize debug schema and typed evidence model
2. persist per-turn evidence to SQLite
3. surface evidence in timeline details
4. add focused debug inspector surface
5. add development-only `tracing` bridge and `tokio-console` support

This ordering ensures replayable evidence is useful before deeper runtime instrumentation lands.

## Alternatives Considered

### Approach B: Runtime-first diagnostics

Strength:

1. Faster path to diagnosing async scheduling issues.

Why not chosen:

1. It underserves the main requirement of AI-friendly replay and postmortem debugging.
2. It would improve developer debugging, but not the model’s ability to reconstruct a bad turn.

### Approach C: Full flight recorder / debug workspace

Strength:

1. Highest ceiling and best long-term debugging power.

Why not chosen:

1. Too much scope for one implementation cycle.
2. High risk of turning the product into a debug dashboard instead of a coding assistant.

## Review Checklist (Self-Target)

1. The design keeps state ownership in Blazar-owned types.
2. The design favors actionable debugging surfaces over passive dashboards.
3. The design is incremental and focused enough for a single implementation plan.
4. The chosen approach clearly prioritizes replayable evidence for AI debugging.
