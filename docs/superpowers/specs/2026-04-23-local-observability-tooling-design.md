# Local Observability Tooling Design (Log-First, Extensible)

## Scope

This design improves Blazar observability for **local single-machine development** with a log-first approach, while reserving clean extension points for future tracing/centralized platforms.

## Goals

1. Make runtime issues quickly diagnosable from local logs.
2. Standardize log structure so filtering and correlation are reliable.
3. Provide low-friction local tooling (`jq`, `lnav`, optional `fzf`) and command wrappers.
4. Preserve forward compatibility with OpenTelemetry/Loki/Grafana without reworking core log call sites.

## Non-Goals

1. No mandatory centralized logging backend in this phase.
2. No production cluster deployment design.
3. No rewrite of runtime architecture; only observability surface hardening.

## Context

Current Blazar already writes rotating file logs (`flexi_logger`) under `logs/`. It has broad `log::{info,warn,debug,trace}` usage across runtime, scheduler, chat/event loop, and provider paths, but logs are primarily human-formatted lines and not standardized around queryable fields.

## Chosen Approach

Hybrid path (**C**):

1. Implement the lightweight local baseline first (structured JSON logs + local viewers).
2. Add explicit correlation fields and a small abstraction seam so future tracing backends can be attached incrementally.

## Architecture

### 1. Log Production Layer (in-app)

Keep `log + flexi_logger`, switch formatter output to JSON lines (one event per line), and standardize event fields.

Required stable fields:

- `ts`
- `level`
- `target`
- `event`
- `message`
- `trace_id`
- `turn_id`
- `tool_name`
- `agent_id`
- `error_kind`

Field values may be `null` when unavailable, but keys should remain stable.

### 2. Local Inspection Layer (developer UX)

Install and use:

- Required: `jq`, `lnav`
- Recommended: `fzf`

Provide workflow commands via `just`:

- `just logs-tail`
- `just logs-errors`
- `just logs-turn <turn_id>`

These commands are the default local diagnosis entrypoints.

### 3. Extension Layer (future-ready seam)

Introduce a narrow correlation/telemetry seam (types/helpers) that keeps `trace_id/span_id` semantics explicit. This seam must be usable by future OpenTelemetry exporters without changing existing call sites that emit runtime/tool/scheduler logs.

## Data Flow

1. Runtime/chat/tool/provider events produce structured log records.
2. Records are written to rotated local files under `logs/`.
3. `just` observability commands filter/query those records by level/event/turn/tool.
4. Developer iterates from high-level error view (`logs-errors`) to scoped trace (`logs-turn`).

## Error Handling Rules

1. Logger initialization failure remains visible immediately (stderr), and should emit a structured fallback diagnostic if file logger starts partially.
2. Missing correlation fields are explicit (`null` + clear event identity), never silently omitted in a way that breaks parsers.
3. Log-query helper commands fail loudly and readably on malformed input; no silent “empty success” masking parser failures.

## Tool Installation Design

Add script:

- `scripts/observability/install-tools.sh`

Behavior:

1. Detect package manager (`apt`/`brew`) when present.
2. Check presence of `jq`, `lnav`, and `fzf`.
3. Install missing tools when supported; otherwise print actionable manual commands.
4. Exit non-zero only on actionable failures (e.g., package command failure), not when optional tooling is skipped by user choice.

## Testing Strategy

1. **Unit tests**
   - Structured log formatter emits required keys and valid JSON per line.
   - Correlation field mapping correctness (`turn_id`, `trace_id`, `error_kind`).

2. **Integration tests**
   - Simulate a failed tool/runtime path; verify `logs-errors` query surfaces the event.
   - Simulate turn-scoped flow; verify `logs-turn <id>` isolates relevant entries.

3. **Command behavior tests**
   - No-log file case.
   - Malformed-log line case.
   - Normal mixed-level log case.

## Acceptance Criteria (MVP)

1. Default local logs are structured JSON lines with required stable keys.
2. Developers can identify one failed turn/tool context from logs within a short workflow (`logs-errors` -> `logs-turn`).
3. Installation script checks/installs local tooling and reports status clearly.
4. Tracing backend adoption later does not require rewriting existing event-log call sites.

## Risks and Mitigations

1. **Risk:** JSON logs reduce ad-hoc readability.
   - **Mitigation:** Keep `lnav` and helper commands as first-class UX.

2. **Risk:** Field drift across modules.
   - **Mitigation:** Centralize formatter + schema tests for key stability.

3. **Risk:** Over-designing for future tracing now.
   - **Mitigation:** Keep extension seam minimal and local; no heavy backend dependency in this phase.

## Implementation Boundaries

Changes should focus on observability interfaces and developer workflow support, without moving product state ownership away from Blazar runtime/app state types.
