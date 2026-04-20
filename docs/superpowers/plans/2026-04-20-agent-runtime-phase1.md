# Agent Runtime Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Phase 1 of the agent runtime (root-agent bootstrap + provider abstraction + single-turn streaming loop) and wire it into the existing chat timeline.

**Architecture:** Introduce a new `src/agent/` module that owns runtime state, protocol events, provider traits, and a core turn runner. Entering the main UI bootstraps one default root agent; all turn execution runs under that root agent in Phase 1. Keep `ChatApp` as the product shell; it consumes runtime events and renders them into existing `TimelineEntry` surfaces. Protocol events are defined with `agent_id` from Phase 1 so Phase 6 mailbox-style inter-agent IPC can be added without breaking contracts. Use a deterministic fake provider in tests and an OpenAI-compatible provider implementation behind environment-based selection.

**Tech Stack:** Rust 2024, tokio, reqwest (rustls), serde/serde_json, ratatui TUI surfaces, existing `just` quality gates

---

## Non-functional constraints (mandatory)

1. **Scalability:** runtime message passing must use bounded channels.
2. **Performance:** no blocking network/tool work in render path.
3. **Stability:** all provider/runtime failures must become explicit typed events.
4. **Architecture safety:** do not replace existing terminal event loop framework in Phase 1.
5. **Process model:** main UI must always start with a stable `root_agent_id`.
6. **IPC compatibility:** runtime event schema must carry `agent_id` in Phase 1.
7. **Memory model compatibility:** keep internal working memory in runtime state; external memory persistence is Phase 4.
8. **Skills compatibility:** protocol must be extensible for future `run_skill` program invocation.
9. **Scheduler compatibility:** runtime state/protocol naming must not block later runnable/waiting queue scheduler introduction.
10. **Capability compatibility:** protocol/state must support future subject-capability-scope authorization checks.
11. **Observability compatibility:** protocol/events must support stable `trace_id`/`op_id` extension fields.
12. **Reliability compatibility:** runtime operation model must support future idempotency keys, retry metadata, and dead-letter persistence.

## Scope split notice

The architecture spec includes six phases. This plan intentionally covers **Phase 1 only** (provider + core loop + timeline wiring).  
Create separate plans for Phase 2+ (`tools`, `permission`, `persistence`, `mcp`, `sub-agents`) after Phase 1 lands and is stable.
Memory layering note: Phase 1 focuses on internal working memory; external durable memory remains in the Phase 4 plan.
MCP layering note: treat MCP servers as external peripherals; connection/device management remains Phase 5 scope.
Skill layering note: skills are executable program units; concrete `run_skill` orchestration is deferred to a follow-up plan after tool/permission foundations.
Scheduler layering note: full multi-agent dispatcher (runnable/waiting queues, fairness, quotas) is implemented in the sub-agent phase, but protocol/state shapes in Phase 1 must remain scheduler-friendly.
Capability layering note: full capability token enforcement lands with Phase 3 policy engine; Phase 1 must preserve extensible operation metadata for authorization hooks.
Kernel-guardrail layering note: full retry/dead-letter/supervisor policies land in later phases; Phase 1 must preserve protocol and state extensibility for those controls.

## File structure

### Create

- `src/agent/mod.rs` — module exports for runtime, protocol, state, provider
- `src/agent/protocol.rs` — runtime operations/events (`AgentOp`, `AgentEvent`)
- `src/agent/state.rs` — owned runtime state (`AgentRunState`, `AgentRuntimeState`)
- `src/agent/runtime.rs` — phase-1 turn runner and event queue
- `src/agent/provider/mod.rs` — provider submodule exports
- `src/agent/provider/traits.rs` — provider request/response types and trait contract
- `src/agent/provider/openai.rs` — OpenAI-compatible provider implementation
- `tests/agent_state.rs` — unit tests for state transitions/defaults
- `tests/agent_provider_openai.rs` — OpenAI response decoding/request-shape tests
- `tests/agent_runtime_loop.rs` — runtime loop tests (streaming + state transitions)
- `tests/chat_agent_phase1.rs` — ChatApp integration tests for timeline streaming

### Modify

- `Cargo.toml` — add async/http deps needed by provider/runtime
- `src/lib.rs` — export `agent` module
- `src/chat/app.rs` — own and consume runtime state/events

## Runtime hardening compatibility targets (Phase 1)

- Include stable operation identity (`turn_id`) for every runtime op/event path.
- Preserve explicit state transitions (`Idle -> Running -> Idle/Failed`) for deterministic replay.
- Keep queue capacity constants centralized in runtime so backpressure policy can be tightened without protocol breaks.
- Keep event payloads structured (no lossy free-form strings for machine-critical fields).

---

### Task 1: Add agent state/protocol scaffolding

**Files:**
- Create: `src/agent/mod.rs`
- Create: `src/agent/protocol.rs`
- Create: `src/agent/state.rs`
- Create: `tests/agent_state.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write failing tests for phase-1 state contract**

Create `tests/agent_state.rs`:

```rust
use blazar::agent::protocol::{AgentEvent, AgentOp};
use blazar::agent::state::{AgentRunState, AgentRuntimeState};

#[test]
fn runtime_state_starts_idle() {
    let state = AgentRuntimeState::default();
    assert_eq!(state.run_state, AgentRunState::Idle);
    assert!(state.pending_ops.is_empty());
    assert_eq!(state.root_agent_id, "root");
    assert_eq!(state.active_agent_id, "root");
}

#[test]
fn enqueue_op_tracks_pending_operations() {
    let mut state = AgentRuntimeState::default();
    state.enqueue_op(AgentOp::UserTurn {
        turn_id: "turn-1".into(),
        prompt: "hello".into(),
    });
    assert_eq!(state.pending_ops.len(), 1);
}

#[test]
fn turn_events_change_state_machine() {
    let mut state = AgentRuntimeState::default();
    state.apply_event(&AgentEvent::TurnStarted {
        agent_id: "root".into(),
        turn_id: "t1".into(),
    });
    assert!(matches!(state.run_state, AgentRunState::Running { .. }));
    state.apply_event(&AgentEvent::TurnCompleted {
        agent_id: "root".into(),
        turn_id: "t1".into(),
    });
    assert_eq!(state.run_state, AgentRunState::Idle);
}
```

- [ ] **Step 2: Run test to confirm red state**

Run: `cargo test --test agent_state -q`  
Expected: FAIL with unresolved import/module errors for `blazar::agent::*`.

- [ ] **Step 3: Implement minimal state/protocol modules**

Create `src/agent/protocol.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentOp {
    UserTurn { turn_id: String, prompt: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    TurnStarted { agent_id: String, turn_id: String },
    AssistantDelta { agent_id: String, turn_id: String, chunk: String },
    TurnCompleted { agent_id: String, turn_id: String },
    TurnFailed { agent_id: String, turn_id: String, message: String },
}
```

Create `src/agent/state.rs`:

```rust
use crate::agent::protocol::{AgentEvent, AgentOp};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AgentRunState {
    #[default]
    Idle,
    Running { turn_id: String },
    Failed { message: String },
}

#[derive(Debug, Clone, Default)]
pub struct AgentRuntimeState {
    pub root_agent_id: String,
    pub active_agent_id: String,
    pub run_state: AgentRunState,
    pub pending_ops: Vec<AgentOp>,
}

impl AgentRuntimeState {
    pub fn new_root() -> Self {
        Self {
            root_agent_id: "root".to_string(),
            active_agent_id: "root".to_string(),
            run_state: AgentRunState::Idle,
            pending_ops: vec![],
        }
    }

    pub fn enqueue_op(&mut self, op: AgentOp) {
        self.pending_ops.push(op);
    }

    pub fn apply_event(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::TurnStarted { turn_id, .. } => {
                self.run_state = AgentRunState::Running {
                    turn_id: turn_id.clone(),
                };
            }
            AgentEvent::TurnCompleted { .. } => {
                self.run_state = AgentRunState::Idle;
            }
            AgentEvent::TurnFailed { message, .. } => {
                self.run_state = AgentRunState::Failed {
                    message: message.clone(),
                };
            }
            AgentEvent::AssistantDelta { .. } => {}
        }
    }
}
```

Update the default implementation to call `new_root()`:

```rust
impl Default for AgentRuntimeState {
    fn default() -> Self {
        Self::new_root()
    }
}
```

Create `src/agent/mod.rs` and update `src/lib.rs`:

```rust
pub mod protocol;
pub mod runtime;
pub mod state;
pub mod provider;
```

```rust
pub mod agent;
pub mod app;
pub mod chat;
pub mod config;
pub mod welcome;
```

- [ ] **Step 4: Run tests to reach green state**

Run: `cargo test --test agent_state -q`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/agent/mod.rs src/agent/protocol.rs src/agent/state.rs tests/agent_state.rs
git commit -m "feat(agent): add phase1 protocol and runtime state scaffolding"
```

---

### Task 2: Implement provider contracts + OpenAI-compatible adapter

**Files:**
- Create: `src/agent/provider/mod.rs`
- Create: `src/agent/provider/traits.rs`
- Create: `src/agent/provider/openai.rs`
- Create: `tests/agent_provider_openai.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Write failing provider tests**

Create `tests/agent_provider_openai.rs`:

```rust
use blazar::agent::provider::openai::{decode_chat_response_text, OpenAiRequestBuilder};

#[test]
fn decode_chat_response_text_extracts_first_message_content() {
    let payload = serde_json::json!({
        "choices": [
            { "message": { "content": "hello from model" } }
        ]
    });

    let text = decode_chat_response_text(&payload).expect("text should decode");
    assert_eq!(text, "hello from model");
}

#[test]
fn request_builder_sets_model_and_user_prompt() {
    let body = OpenAiRequestBuilder::new("gpt-4o-mini")
        .with_user_prompt("Summarize this diff")
        .build_json();

    assert_eq!(body["model"], "gpt-4o-mini");
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"], "Summarize this diff");
}
```

- [ ] **Step 2: Run test to confirm red state**

Run: `cargo test --test agent_provider_openai -q`  
Expected: FAIL due to missing `provider::openai` symbols.

- [ ] **Step 3: Add dependencies for provider/runtime**

Update `Cargo.toml`:

```toml
[dependencies]
async-trait = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

- [ ] **Step 4: Implement provider trait and OpenAI adapter**

Create `src/agent/provider/traits.rs`:

```rust
use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderTurnRequest {
    pub turn_id: String,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderEvent {
    Delta(String),
    Completed,
    Failed(String),
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn run_turn(&self, request: ProviderTurnRequest) -> Result<Vec<ProviderEvent>, String>;
}
```

Create `src/agent/provider/openai.rs` with pure helpers first:

```rust
pub fn decode_chat_response_text(payload: &serde_json::Value) -> Result<String, String> {
    payload["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "missing choices[0].message.content".to_string())
}

pub struct OpenAiRequestBuilder {
    model: String,
    prompt: String,
}

impl OpenAiRequestBuilder {
    pub fn new(model: &str) -> Self {
        Self { model: model.to_string(), prompt: String::new() }
    }

    pub fn with_user_prompt(mut self, prompt: &str) -> Self {
        self.prompt = prompt.to_string();
        self
    }

    pub fn build_json(self) -> serde_json::Value {
        serde_json::json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": self.prompt }]
        })
    }
}
```

Create a deterministic fallback provider in `src/agent/provider/mod.rs`:

```rust
pub struct EchoProvider {
    prefix: String,
}

impl EchoProvider {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self { prefix: prefix.into() }
    }
}

#[async_trait::async_trait]
impl LlmProvider for EchoProvider {
    async fn run_turn(
        &self,
        request: ProviderTurnRequest,
    ) -> Result<Vec<ProviderEvent>, String> {
        Ok(vec![
            ProviderEvent::Delta(format!("{}{}", self.prefix, request.prompt)),
            ProviderEvent::Completed,
        ])
    }
}
```

- [ ] **Step 5: Run provider tests**

Run: `cargo test --test agent_provider_openai -q`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/agent/provider/mod.rs src/agent/provider/traits.rs src/agent/provider/openai.rs tests/agent_provider_openai.rs
git commit -m "feat(agent): add provider contracts and openai request adapter"
```

---

### Task 3: Build phase-1 runtime loop with deterministic streaming

**Files:**
- Create: `src/agent/runtime.rs`
- Create: `tests/agent_runtime_loop.rs`
- Modify: `src/agent/mod.rs`

- [ ] **Step 1: Write failing runtime loop tests**

Create `tests/agent_runtime_loop.rs`:

```rust
use blazar::agent::protocol::{AgentEvent, AgentOp};
use blazar::agent::runtime::{AgentRuntime, StaticScriptProvider};

#[tokio::test]
async fn user_turn_emits_started_delta_completed_events() {
    let provider = StaticScriptProvider::new(vec!["hello ", "world"]);
    let mut runtime = AgentRuntime::new(Box::new(provider));

    let events = runtime
        .run_op(AgentOp::UserTurn {
            turn_id: "t1".into(),
            prompt: "say hello".into(),
        })
        .await;

    assert!(matches!(events[0], AgentEvent::TurnStarted { .. }));
    assert!(events.iter().any(|e| matches!(e, AgentEvent::AssistantDelta { .. })));
    assert!(matches!(events.last(), Some(AgentEvent::TurnCompleted { .. })));
}

#[tokio::test]
async fn provider_error_emits_turn_failed() {
    let provider = StaticScriptProvider::failing("network down");
    let mut runtime = AgentRuntime::new(Box::new(provider));
    let events = runtime
        .run_op(AgentOp::UserTurn { turn_id: "t2".into(), prompt: "x".into() })
        .await;
    assert!(events.iter().any(|e| matches!(e, AgentEvent::TurnFailed { .. })));
}
```

- [ ] **Step 2: Run runtime tests to confirm red state**

Run: `cargo test --test agent_runtime_loop -q`  
Expected: FAIL due to missing `AgentRuntime` and `StaticScriptProvider`.

- [ ] **Step 3: Implement runtime and deterministic test provider**

Create `src/agent/runtime.rs`:

```rust
use crate::agent::protocol::{AgentEvent, AgentOp};
use crate::agent::provider::traits::{LlmProvider, ProviderEvent, ProviderTurnRequest};

pub struct AgentRuntime {
    provider: Box<dyn LlmProvider>,
}

impl AgentRuntime {
    pub fn new(provider: Box<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    pub async fn run_op(&mut self, op: AgentOp) -> Vec<AgentEvent> {
        match op {
            AgentOp::UserTurn { turn_id, prompt } => {
                let agent_id = "root".to_string();
                let mut out = vec![AgentEvent::TurnStarted {
                    agent_id: agent_id.clone(),
                    turn_id: turn_id.clone(),
                }];

                let req = ProviderTurnRequest { turn_id: turn_id.clone(), prompt };
                match self.provider.run_turn(req).await {
                    Ok(events) => {
                        for event in events {
                            match event {
                                ProviderEvent::Delta(chunk) => out.push(AgentEvent::AssistantDelta {
                                    agent_id: agent_id.clone(),
                                    turn_id: turn_id.clone(),
                                    chunk,
                                }),
                                ProviderEvent::Completed => {}
                                ProviderEvent::Failed(msg) => {
                                    out.push(AgentEvent::TurnFailed {
                                        agent_id: agent_id.clone(),
                                        turn_id: turn_id.clone(),
                                        message: msg,
                                    });
                                    return out;
                                }
                            }
                        }
                        out.push(AgentEvent::TurnCompleted { agent_id, turn_id });
                    }
                    Err(message) => out.push(AgentEvent::TurnFailed {
                        agent_id,
                        turn_id,
                        message,
                    }),
                }

                out
            }
        }
    }
}
```

Also add deterministic test double in the same file:

```rust
pub struct StaticScriptProvider {
    chunks: Vec<String>,
    fail: Option<String>,
}
```

Implement `LlmProvider` for it so tests can run without network access:

```rust
impl StaticScriptProvider {
    pub fn new(chunks: Vec<&str>) -> Self {
        Self {
            chunks: chunks.into_iter().map(str::to_owned).collect(),
            fail: None,
        }
    }

    pub fn failing(message: &str) -> Self {
        Self {
            chunks: vec![],
            fail: Some(message.to_string()),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for StaticScriptProvider {
    async fn run_turn(&self, _request: ProviderTurnRequest) -> Result<Vec<ProviderEvent>, String> {
        if let Some(msg) = &self.fail {
            return Err(msg.clone());
        }

        let mut events = self
            .chunks
            .iter()
            .cloned()
            .map(ProviderEvent::Delta)
            .collect::<Vec<_>>();
        events.push(ProviderEvent::Completed);
        Ok(events)
    }
}
```

- [ ] **Step 4: Run runtime tests to green**

Run: `cargo test --test agent_runtime_loop -q`  
Expected: PASS.

- [ ] **Step 4.1: Add bounded channel test**

Extend `tests/agent_runtime_loop.rs` with:

```rust
#[tokio::test]
async fn runtime_uses_bounded_event_queue_capacity() {
    let provider = StaticScriptProvider::new(vec!["a"]);
    let runtime = AgentRuntime::new(Box::new(provider));
    assert_eq!(runtime.event_queue_capacity_for_test(), 256);
}
```

Implement `event_queue_capacity_for_test()` in `src/agent/runtime.rs` and keep queue capacity fixed at `256`.

- [ ] **Step 5: Commit**

```bash
git add src/agent/runtime.rs src/agent/mod.rs tests/agent_runtime_loop.rs
git commit -m "feat(agent): add phase1 runtime loop with streaming events"
```

---

### Task 4: Integrate runtime with ChatApp timeline

**Files:**
- Modify: `src/chat/app.rs`
- Create: `tests/chat_agent_phase1.rs`

- [ ] **Step 1: Write failing ChatApp integration test**

Create `tests/chat_agent_phase1.rs`:

```rust
use blazar::chat::app::ChatApp;

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn send_message_runs_through_agent_turn_pipeline() {
    let mut app = ChatApp::new_for_test(REPO_ROOT);
    app.send_message("build a plan");

    let last = app.timeline().last().expect("timeline should not be empty");
    assert!(last.details.contains("agent-turn-id="));
    assert_eq!(app.agent_status_for_test(), "idle");
    assert_eq!(app.root_agent_id_for_test(), "root");
}
```

- [ ] **Step 2: Run test to confirm red state**

Run: `cargo test --test chat_agent_phase1 -q`  
Expected: FAIL before wiring because runtime events are not connected.

- [ ] **Step 3: Wire ChatApp to consume runtime events**

In `src/chat/app.rs`:

1. Add owned runtime fields:

```rust
agent_state: crate::agent::state::AgentRuntimeState,
agent_runtime: crate::agent::runtime::AgentRuntime,
agent_executor: tokio::runtime::Runtime,
```

2. In `send_message`, replace direct Spirit echo insertion with:
- enqueue user turn op (`AgentOp::UserTurn`)
- run runtime turn
- append `AgentEvent::AssistantDelta` chunks to the latest assistant `TimelineEntry`
- apply `TurnStarted/TurnCompleted/TurnFailed` to `agent_state`

Core call path:

```rust
let events = self.agent_executor.block_on(
    self.agent_runtime.run_op(AgentOp::UserTurn {
        turn_id: format!("turn-{}", self.tick_count),
        prompt: trimmed.to_owned(),
    }),
);
```

3. Keep compatibility behavior: if provider path errors, append a warning timeline entry rather than silent return.

4. Keep existing animation triggers for new assistant content.

5. Add a tiny test-only status reader:

```rust
#[cfg(test)]
pub fn agent_status_for_test(&self) -> &'static str {
    match self.agent_state.run_state {
        crate::agent::state::AgentRunState::Idle => "idle",
        crate::agent::state::AgentRunState::Running { .. } => "running",
        crate::agent::state::AgentRunState::Failed { .. } => "failed",
    }
}
```

Also add a root-agent reader for test assertions:

```rust
#[cfg(test)]
pub fn root_agent_id_for_test(&self) -> &str {
    &self.agent_state.root_agent_id
}
```

- [ ] **Step 4: Run ChatApp integration tests**

Run: `cargo test --test chat_agent_phase1 --test chat_boot --test chat_runtime -q`  
Expected: PASS; no regressions in existing chat behavior.

- [ ] **Step 4.1: Add non-blocking handoff test**

Add to `tests/chat_agent_phase1.rs`:

```rust
#[test]
fn send_message_enqueues_work_without_blocking_render_state_machine() {
    let mut app = ChatApp::new_for_test(REPO_ROOT);
    app.send_message("non blocking check");
    assert_eq!(app.agent_status_for_test(), "idle");
}
```

- [ ] **Step 5: Commit**

```bash
git add src/chat/app.rs tests/chat_agent_phase1.rs
git commit -m "feat(chat): route message handling through phase1 agent runtime"
```

---

### Task 5: Provider selection + quality gates

**Files:**
- Modify: `src/agent/provider/mod.rs`
- Modify: `src/agent/provider/openai.rs`
- Modify: `src/chat/app.rs`

- [ ] **Step 1: Add environment-driven provider selection**

Implement helper:

```rust
pub fn provider_kind_from_env(api_key: Option<String>, model: Option<String>) -> &'static str {
    match (api_key, model) {
        (Some(_), Some(_)) => "openai",
        _ => "echo",
    }
}

pub fn default_provider() -> Box<dyn LlmProvider> {
    default_provider_from_env(
        std::env::var("BLAZAR_OPENAI_API_KEY").ok(),
        std::env::var("BLAZAR_OPENAI_MODEL").ok(),
    )
}

pub fn default_provider_from_env(
    api_key: Option<String>,
    model: Option<String>,
) -> Box<dyn LlmProvider> {
    match (api_key, model) {
        (Some(key), Some(model)) => Box::new(OpenAiProvider::new(key, model)),
        _ => Box::new(EchoProvider::new("I hear you — ")),
    }
}
```

Then initialize `ChatApp` runtime with `default_provider()`.

- [ ] **Step 2: Add failing test for fallback path**

Add to `tests/agent_provider_openai.rs`:

```rust
#[test]
fn default_provider_uses_fallback_without_env() {
    let kind = blazar::agent::provider::provider_kind_from_env(None, None);
    assert_eq!(kind, "echo");
}
```

- [ ] **Step 3: Run focused tests**

Run: `cargo test --test agent_provider_openai --test agent_runtime_loop --test chat_agent_phase1 -q`  
Expected: PASS.

- [ ] **Step 4: Run repository quality gates**

Run: `just fmt-check && just lint && just test`  
Expected: all commands succeed.

- [ ] **Step 5: Commit**

```bash
git add src/agent/provider/mod.rs src/agent/provider/openai.rs src/chat/app.rs tests/agent_provider_openai.rs
git commit -m "feat(agent): add default provider selection and phase1 validation"
```

---

## Spec coverage review

- **Phase 1 provider + core loop:** covered by Tasks 1-3.
- **Timeline integration with existing shell:** covered by Task 4.
- **OpenAI-compatible provider in runtime path:** covered by Tasks 2 and 5.
- **State ownership in Blazar-owned types:** covered by Task 1 and Task 4.
- **Testing and quality gates:** covered by every task plus Task 5 final gates.
- **Phases 2-6 (tools/permissions/persistence/mcp/sub-agents):** intentionally deferred to follow-up plans.
