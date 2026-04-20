# Blazar Agent Runtime Architecture Design

**Date:** 2026-04-20  
**Status:** Proposed

## Problem

Blazar's TUI is now stable enough to shift priority from presentation to workflow depth. The next major gap is the **agent runtime**: model interaction, tool execution, permission safety, state continuity, and MCP extensibility.

Current code has a strong UI shell (`chat/app.rs`, `chat/view/*`, `chat/event_loop.rs`) but no production agent control plane behind it.

## Design Goals

1. Keep Blazar as a **Codex-like terminal coding assistant** (not a generic chat client).
2. Keep core state in **Blazar-owned state types**, not widget state.
3. Ship an incremental architecture that can start useful and grow safely.
4. Make risky actions safe by default (approval + preview + explicit confirmation).
5. Preserve compatibility with current TUI timeline and status surfaces.
6. Optimize for **scalability, performance, and stability** from the baseline architecture.

## Research Synthesis (Codex CLI / Hermes / Oh My Codex)

| System | Pattern to borrow | Why it matters for Blazar |
|---|---|---|
| Codex CLI | Session + Op/Event protocol + orchestrated tool approvals | Proven shape for reason/act/observe loop with safety gates |
| Codex CLI | ToolOrchestrator (approval → sandbox → execute → retry) | Clear, auditable risk control before command execution |
| Hermes | Trait-based provider/tool abstractions | Allows clean provider growth without rewiring runtime |
| Hermes | Streaming-first loop + parallel tool dispatch | Better responsiveness and throughput for coding tasks |
| Oh My Codex | Durable workflow state and role-aware orchestration | Improves long-running task continuity and sub-agent scalability |
| Oh My Codex | MCP-first tool federation | Standard extension path without tight coupling |

## Approach Options

### Option A — Codex-parity monolith (single large runtime module)

Implement provider, loop, tools, permissions, persistence, and MCP together in one subsystem.

**Pros:** Fastest path to broad feature parity on paper.  
**Cons:** High risk, weak boundaries, hard to test incrementally, violates workflow-first delivery discipline.

### Option B — Staged modular runtime in current crate (**Recommended**)

Add a new `agent` module tree with strict boundaries and ship in phases:
1) provider + loop, 2) tools, 3) permission/sandbox, 4) persistence, 5) MCP, 6) sub-agents.

**Pros:** Best fit for Blazar standards, lower delivery risk, strong testability, preserves state ownership.  
**Cons:** Requires discipline on interfaces before feature expansion.

### Option C — Orchestration-first (OMX-like role system first)

Start with multi-agent/team orchestration, mode routing, and role metadata before robust core runtime.

**Pros:** Fast visible "multi-agent" story.  
**Cons:** Weak foundation; brittle without solid single-agent loop and safety controls.

## Chosen Approach

Use **Option B (staged modular runtime)**.

This gives Blazar a stable single-agent foundation first, then adds tools, safety, persistence, and MCP in an implementation order consistent with coding standards.

## Target Architecture

### Module Layout (initial shape)

```text
src/agent/
  mod.rs
  runtime.rs          # turn lifecycle + orchestrator entrypoint
  protocol.rs         # runtime events and turn operations
  state.rs            # Blazar-owned agent state models
  provider/
    mod.rs
    traits.rs         # LlmProvider, stream event abstractions
    openai.rs         # first concrete provider
  tools/
    mod.rs
    registry.rs       # ToolSpec registration
    dispatch.rs       # sequential/parallel dispatch policy
    builtins.rs       # first-party tools exposed to model
  permission/
    mod.rs
    policy.rs         # approval modes/rules
    approval_store.rs # session-level caching
  session/
    mod.rs
    store.rs          # sqlite persistence
    context.rs        # context window and compaction policy
  mcp/
    mod.rs
    client.rs         # MCP server connection + tool import
```

### Integration boundary with existing TUI

- `chat/app.rs` remains product shell owner.
- Add `agent_state: AgentRuntimeState` to `ChatApp` (or an adjacent Blazar-owned app state wrapper).
- Timeline rendering continues to use `TimelineEntry`; runtime events are translated into timeline entries.
- Event loop keeps rendering and input ownership; agent runtime runs as async task with channel-based events.
- Entering the main UI bootstraps one default **root agent**.

### Agent Process Model (Root + Child)

Blazar uses an OS-like process tree model for agents:

- **Root agent**: created at session entry, owns the primary user conversation.
- **Child agents**: created by agent calls (delegation), with parent/child linkage.
- **Agent-as-tool**: spawning/waiting/messaging child agents is exposed as tool operations to agents.

This keeps one unified execution model: tools are invocations, and agent delegation is a specialized invocation with stronger lifecycle/state controls.

### Agent Orchestration Scheduler Model

Multi-agent orchestration is explicitly a **scheduler system**:

- Scheduler owns runnable/waiting/blocked/completed agent queues.
- Scheduler decides execution order (initially FIFO + priority hints).
- Scheduler enforces concurrency limits, depth limits, and resource caps.
- Scheduler coordinates IPC mailbox delivery and wake-up conditions.

Conceptually:
- agent = process
- scheduler = kernel dispatcher
- tools/skills = executable work units

This gives Blazar deterministic, observable control over multiple agent executions.

### Performance and Stability Baseline

- Keep the existing terminal event loop; do not replace it with a framework takeover.
- Run provider/tool work off the render loop (Tokio tasks + channels).
- Use bounded queues for backpressure to prevent unbounded memory growth.
- Coalesce high-frequency runtime events before render when possible.
- Keep append-only runtime event persistence to support replay and failure recovery.
- Treat provider/tool/network failures as typed events, not silent retries.

### Core State Types

```rust
pub enum AgentRunState {
    Idle,
    Running { turn_id: String },
    WaitingApproval { request_id: String },
    RunningTool { tool_name: String },
    Failed { message: String },
}

pub struct AgentRuntimeState {
    pub root_agent_id: String,
    pub active_agent_id: String,
    pub run_state: AgentRunState,
    pub active_model: String,
    pub pending_ops: Vec<AgentOp>,
    pub last_token_usage: Option<TokenUsage>,
}
```

All state above is Blazar-owned and independent from rendering helpers.

### Agent Loop (root-agent baseline)

Turn lifecycle:
1. Build turn context (cwd, user prompt, relevant history, policy mode).
2. Stream provider response.
3. Convert model output into operations:
   - assistant text
   - tool invocation request
   - terminal turn-complete metadata
4. Execute approved tools and append tool outputs to context.
5. Continue until model emits final response or failure.

This loop will start with sequential tool execution, then optional bounded parallelism once tool safety and state replay are solid.

### Provider Abstraction

`LlmProvider` trait supports both non-stream and stream mode; stream mode is primary.

```rust
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn stream_turn(
        &self,
        req: ProviderTurnRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<ProviderEvent>, ProviderError>;
}
```

Initial provider: OpenAI-compatible chat endpoint.  
Follow-up provider: Anthropic-compatible endpoint.  
Provider selection stays in Blazar config state, not hardcoded in views.

### Tool System

### Registry

Tools are declared as `ToolSpec` (name, schema, risk metadata, handler id).  
Model sees only tools registered in current policy context.

### Dispatch

Dispatch pipeline:
1. validate args against schema
2. permission check (policy + approval store)
3. execute tool
4. normalize output envelope
5. emit timeline/runtime event

### Built-in v1 tools

- `bash` (bounded command execution, explicit cwd)
- `view` (file reads)
- `rg` (repo search)
- `glob` (file discovery)

`write/apply_patch` should remain approval-gated from day one.

### Agent-as-tool operations

Agent delegation is represented as first-class tool operations:

- `spawn_agent` (create child agent with prompt + scope)
- `wait_agent` (await status/completion)
- `write_agent` (send follow-up input)
- `cancel_agent` (terminate child execution)

These operations go through the same policy/approval/audit pipeline as other risky tools.

### Skills Program Model

Skills are treated as **executable program units**:

- A skill is a named program contract (`name`, `inputs`, `runtime`, `expected outputs`).
- Skills can orchestrate multiple tool calls and agent calls as one reusable routine.
- Agents invoke skills via `run_skill` the same way they invoke tools.
- Skill execution is observable (start/step/finish events) and auditable.

Execution model:
- compile-time declaration: skill metadata + schema
- runtime invocation: validated args + policy checks
- deterministic output envelope returned to caller agent

In short: **tools are primitive syscalls; skills are reusable programs built from those primitives.**

### Inter-Agent IPC Model (Mailbox-style)

Agent-to-agent communication uses an IPC-style mailbox abstraction:

- Every agent has an inbox mailbox identified by `agent_id`.
- `write_agent` appends a message envelope to a target mailbox.
- Runtime scheduler delivers mailbox messages as ordered `AgentOp` inputs.
- `wait_agent` can block on terminal status or next IPC event (with timeout).

Message envelope shape (runtime-level):

```rust
pub struct AgentMessageEnvelope {
    pub from_agent_id: String,
    pub to_agent_id: String,
    pub correlation_id: String,
    pub body: String,
    pub sent_at_ms: u64,
}
```

IPC guarantees for v1:
- per-target mailbox FIFO ordering
- at-least-once delivery inside one runtime session
- bounded mailbox capacity with explicit overflow error events

Out of scope for v1:
- distributed cross-process delivery
- exactly-once guarantees across restarts

### Permission and Safety Model

Capability-based authorization model (mandatory):

- **Subject (who):** `root-agent`, `child-agent`, `skill`, `mcp-server`, `user`.
- **Capability (what):** `read_file`, `write_file`, `exec_cmd`, `network`, `spawn_agent`, `run_skill`, `use_mcp`.
- **Scope (where/how much):** path/domain allowlists, TTL, call limits, token/cost budget.
- **Decision (allow/deny/ask):** policy engine output for every risky operation.

Capability token shape:

```yaml
subject: child-agent-42
caps:
  - action: read_file
    scope: ["repo:/home/lx/blazar/**"]
  - action: exec_cmd
    scope: ["cmd:git *", "cmd:cargo test *"]
constraints:
  ttl_sec: 900
  max_tool_calls: 20
  max_tokens: 30000
  allow_network: false
inherits_from: root-agent
```

Approval modes:
- `Never` (trusted local only; still logs actions)
- `OnRisky` (default)
- `Always`

Risk categories:
- `ReadOnly`
- `RepoWrite`
- `CommandExec`
- `Network`
- `Destructive`

Policy engine output:
- allow
- deny
- ask user (with structured reason + scope)

Approval decisions can be cached for session scope, with explicit "always deny/allow" rule support in later phases.

Hard rules:
- Child agents can only receive a subset of parent capabilities (no privilege escalation).
- Skills must declare required minimum capabilities and are checked before execution.
- MCP servers are authorized as external devices with per-device isolation.
- All decisions and executions are audit-logged for replay.

### Session Persistence

Extend SQLite usage with runtime tables (in existing session DB strategy):

- `agent_turns` (turn id, prompt, status, timestamps)
- `agent_events` (event stream for replay/debug)
- `tool_calls` (tool name, args hash, decision, duration, outcome)
- `token_usage` (prompt/completion/total by turn and model)

Persistence is append-oriented to support replay and crash recovery.

### Agent Memory System (Internal + External)

Blazar memory is explicitly split into two layers:

- **Internal memory (working memory)**  
  In-memory, short-lived runtime context used during active turns:
  - recent turn window
  - unresolved tool outputs
  - active IPC mailbox state
  - per-agent execution scratch context

- **External memory (persistent memory)**  
  Durable storage across turns/sessions:
  - turn/event history
  - tool execution history
  - compacted summaries
  - long-lived user/workspace facts

Design rule:
- Internal memory optimizes speed and local reasoning.
- External memory optimizes continuity, recovery, and long-horizon behavior.
- Promotion from internal to external memory is explicit (end-of-turn, compaction, or checkpoint), never implicit side effects.

### Context Management

Start simple:
- fixed max turn history window
- summarize old content into compacted assistant notes
- always retain: system/developer constraints + latest user intent + unresolved tool outcomes

Compaction policy is runtime-owned and testable independently from TUI rendering.

### MCP Integration

MCP is phase-gated, not first release blocker.

MCP is treated as an **external peripheral bus** for Blazar runtime:

- MCP servers are like pluggable external devices.
- Each server exposes capabilities (tools/resources) through a stable protocol boundary.
- Runtime owns device registration, health checks, and capability discovery.
- Device failures are isolated; one failing server must not break core agent loop.

Minimum MCP design:
- connection manager for configured servers
- import tool schemas with namespaced names (`server/tool`)
- route execution through same permission/dispatch pipeline as built-ins

If MCP server fails, degrade gracefully: disable its tools, keep session alive.

### Error Handling

Do not swallow failures. Use typed errors:
- provider error
- parse/protocol error
- policy denial
- tool runtime error
- persistence error

Each error maps to:
1) structured runtime event, 2) user-visible timeline entry, 3) recoverability decision (retry, ask, abort turn).

### Kernel Runtime Guardrails

To align with high-performance and high-stability goals, runtime must enforce:

- **Bounded resources**: hard limits for concurrent agents, tool calls, mailbox depth, and token budgets.
- **Deterministic scheduling hooks**: explicit runnable/waiting/blocked transitions recorded as events.
- **Reliable execution semantics**: idempotency key per operation, bounded retries with backoff, dead-letter queue on exhaustion.
- **Supervisor behavior**: child-agent crash detection, restart policy (when safe), and failure propagation to parent timeline.
- **Checkpoint safety**: explicit checkpoint boundaries before high-risk multi-step actions.
- **Backpressure first**: queue overflow returns typed overload events, never silent drops.

### Engineering Assurance Baseline

- **Observability**: per-turn trace IDs, per-tool latency/cost metrics, scheduler queue metrics.
- **Auditability**: append-only audit log for policy decisions + executions + IPC sends.
- **Reproducibility**: deterministic replay mode for runtime event streams in tests.
- **Release gates**: runtime changes must pass `just fmt-check`, `just lint`, `just test` plus runtime integration tests.
- **Failure drills**: periodic tests for provider timeout, MCP outage, queue overflow, and child-agent crash recovery.

### Testing Strategy

1. Unit tests for:
   - provider event parsing
   - permission policy decisions
   - tool arg validation
   - context compaction
2. Integration tests for:
   - full turn with tool call
   - denied tool path
   - persistence write/read replay
3. Snapshot tests for:
   - timeline presentation of runtime events
4. Regression tests for:
   - approval cache behavior
   - command safety boundaries

## Phased Feature Completion Plan

### Phase 1 — Provider + core loop (MVP foundation)

Deliver:
- `agent` module scaffold
- root agent bootstrap on UI entry
- OpenAI-compatible provider
- single-turn streaming response into timeline
- no external tools yet (text-only assistant + internal events)

Exit criteria:
- user prompt can run through provider and render streaming/final assistant output
- turn status appears in runtime state
- runtime exposes stable `root_agent_id` and active agent bookkeeping

### Phase 2 — Tool registry + built-in read tools

Deliver:
- tool spec/registry
- `view`, `rg`, `glob` handlers
- tool invocation path in loop
- operation idempotency keys and retry envelope primitives

Exit criteria:
- model can call read-only tools and continue response
- tool calls are visible in timeline with status
- repeated operation replay does not duplicate side effects

### Phase 3 — Write/command tools + approvals

Deliver:
- `bash` and `apply_patch` integration
- policy engine + approval prompts
- session-scoped approval cache
- capability token evaluation (subject/capability/scope/constraints)
- parent->child subset capability inheritance checks

Exit criteria:
- risky tools always pass through policy
- deny/ask/allow paths are explicit and logged
- child agents cannot exceed parent privileges

### Phase 4 — Persistence + context compaction

Deliver:
- runtime tables and event append
- history replay on session reload
- basic context compaction
- internal/external memory promotion rules
- dead-letter queue persistence and replay tooling

Exit criteria:
- session can resume with prior turn/tool history
- long sessions remain within configured context budget
- agent memory survives restarts via external memory while keeping hot-path working memory bounded
- exhausted retries are inspectable and recoverable from persisted dead-letter records

### Phase 5 — MCP federation

Deliver:
- MCP server connection manager
- namespaced tool import
- common dispatch/policy path for MCP tools

Exit criteria:
- connected MCP tools are callable in same safety model
- server failure does not crash runtime

### Phase 6 — Sub-agent orchestration (agent process tree)

Deliver:
- spawn/wait/message primitives
- bounded concurrency and depth limits
- parent-child event linking in persistence
- mailbox-based IPC between agents
- supervisor restart/escalation policy for child-agent failures

Exit criteria:
- sub-agent lifecycle observable and cancellable
- parent timeline shows child progress and outcome
- agent-to-agent message flow is observable in runtime events and persisted logs
- child-agent failures follow explicit restart/escalation policy with no silent loss

## Risks and Mitigations

1. **Scope explosion**  
   Mitigation: phase gates with explicit exit criteria and no cross-phase leakage.

2. **State drift into UI helpers**  
   Mitigation: all runtime/session/policy state lives under `src/agent/*` and app-owned state structs.

3. **Unsafe tool execution defaults**  
   Mitigation: default `OnRisky`, explicit risk taxonomy, mandatory policy route for write/exec/network.

4. **Provider lock-in**  
   Mitigation: trait-based provider boundary and provider-neutral protocol events.

## Success Metrics

1. Blazar can complete an end-to-end coding turn with model + tools + approvals.
2. All risky actions are previewed/approved and recorded.
3. Runtime state is resumable across sessions.
4. New providers/tools can be added without rewriting TUI surfaces.
5. `just fmt-check`, `just lint`, `just test` remain green as phases land.
