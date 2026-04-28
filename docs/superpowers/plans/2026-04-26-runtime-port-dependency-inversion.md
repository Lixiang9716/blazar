# Runtime Port Dependency Inversion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make upper-layer chat workflow depend on a runtime trait (`AgentRuntimePort`) instead of concrete `AgentRuntime`, while preserving existing behavior.

**Architecture:** Introduce a thin Chat-facing runtime boundary trait, implement it on existing `AgentRuntime`, and switch `ChatApp` storage/construction to `Box<dyn AgentRuntimePort>`. Keep runtime internals, tool scheduler, provider stack, and UI behavior unchanged.

**Tech Stack:** Rust, ratatui TUI, existing `AgentRuntime` event channel model, cargo test/nextest, just fmt-check/lint/test.

---

## File structure and responsibilities

- Create: `src/chat/runtime_port.rs`
  - Defines Chat-facing runtime abstraction (`AgentRuntimePort`) and test-friendly helpers if needed.
- Modify: `src/chat/mod.rs`
  - Exposes `runtime_port` module.
- Modify: `src/agent/runtime.rs`
  - Implements `AgentRuntimePort` for `AgentRuntime` (no behavior change).
- Modify: `src/chat/app.rs`
  - Replaces concrete runtime field with trait object and updates constructors.
- Modify: `tests/unit/chat/app/tests_impl.inc`
  - Adds boundary-focused tests with fake runtime.

### Task 1: Introduce `AgentRuntimePort` trait and runtime implementation

**Files:**
- Create: `src/chat/runtime_port.rs`
- Modify: `src/chat/mod.rs`
- Modify: `src/agent/runtime.rs`
- Test: `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Write the failing test (trait contract compile check)**

```rust
#[test]
fn agent_runtime_implements_runtime_port_trait() {
    fn assert_port<T: crate::chat::runtime_port::AgentRuntimePort>() {}
    assert_port::<crate::agent::runtime::AgentRuntime>();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib agent_runtime_implements_runtime_port_trait`  
Expected: FAIL because `AgentRuntimePort` does not exist yet.

- [ ] **Step 3: Write minimal implementation**

```rust
// src/chat/runtime_port.rs
pub trait AgentRuntimePort: Send {
    fn submit_turn(&self, prompt: &str) -> Result<(), String>;
    fn set_model(&self, model: &str) -> Result<(), String>;
    fn refresh_acp_agents(&self) -> Result<(), String>;
    fn cancel(&self);
    fn try_recv(&self) -> Option<crate::agent::protocol::AgentEvent>;
}
```

```rust
// src/chat/mod.rs
pub mod runtime_port;
```

```rust
// src/agent/runtime.rs
impl crate::chat::runtime_port::AgentRuntimePort for AgentRuntime {
    fn submit_turn(&self, prompt: &str) -> Result<(), String> {
        AgentRuntime::submit_turn(self, prompt)
    }
    fn set_model(&self, model: &str) -> Result<(), String> {
        AgentRuntime::set_model(self, model)
    }
    fn refresh_acp_agents(&self) -> Result<(), String> {
        AgentRuntime::refresh_acp_agents(self)
    }
    fn cancel(&self) {
        AgentRuntime::cancel(self);
    }
    fn try_recv(&self) -> Option<crate::agent::protocol::AgentEvent> {
        AgentRuntime::try_recv(self)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib agent_runtime_implements_runtime_port_trait`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/runtime_port.rs src/chat/mod.rs src/agent/runtime.rs tests/unit/chat/app/tests_impl.inc
git commit -m "refactor(chat): add AgentRuntimePort boundary trait"
```

### Task 2: Switch `ChatApp` to depend on trait object

**Files:**
- Modify: `src/chat/app.rs`
- Test: `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Write the failing test (inject fake runtime and drain events)**

```rust
#[test]
fn chat_app_tick_drains_events_from_runtime_port() {
    let runtime = FakeRuntime::with_events(vec![crate::agent::protocol::AgentEvent::TurnStarted {
        turn_id: "t-1".into(),
    }]);
    let mut app = ChatApp::new_with_runtime_for_test(
        env!("CARGO_MANIFEST_DIR"),
        Box::new(runtime),
        "echo".to_owned(),
    )
    .expect("app should initialize");
    app.tick();
    assert!(app.is_streaming());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib chat_app_tick_drains_events_from_runtime_port`  
Expected: FAIL because `ChatApp` cannot accept injected runtime yet.

- [ ] **Step 3: Write minimal implementation**

```rust
// src/chat/app.rs (field type)
use crate::chat::runtime_port::AgentRuntimePort;

pub struct ChatApp {
    // ...
    agent_runtime: Box<dyn AgentRuntimePort>,
    // ...
}
```

```rust
// src/chat/app.rs (constructor wiring)
let runtime = AgentRuntime::new(provider, workspace_root.clone(), model_name.clone())?;
agent_runtime: Box::new(runtime),
```

```rust
// src/chat/app.rs (test-only helper)
#[cfg(test)]
pub(crate) fn new_with_runtime_for_test(
    repo_path: &str,
    runtime: Box<dyn AgentRuntimePort>,
    model_name: String,
) -> Result<Self, AgentRuntimeError> {
    let mut app = Self::new(repo_path)?;
    app.agent_runtime = runtime;
    app.model_name = model_name;
    Ok(app)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib chat_app_tick_drains_events_from_runtime_port`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/app.rs tests/unit/chat/app/tests_impl.inc
git commit -m "refactor(chat): make ChatApp depend on AgentRuntimePort"
```

### Task 3: Add boundary-focused behavior tests for runtime calls

**Files:**
- Modify: `tests/unit/chat/app/tests_impl.inc`
- (if needed) Modify: `src/chat/app.rs`

- [ ] **Step 1: Write failing tests for call forwarding**

```rust
#[test]
fn chat_app_cancel_turn_calls_runtime_port_cancel() {
    let runtime = FakeRuntime::streaming();
    let flag = runtime.cancel_called.clone();
    let mut app = ChatApp::new_with_runtime_for_test(
        env!("CARGO_MANIFEST_DIR"),
        Box::new(runtime),
        "echo".to_owned(),
    )
    .expect("app should initialize");
    app.cancel_turn();
    assert!(flag.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn chat_app_set_model_calls_runtime_port_set_model() {
    let runtime = FakeRuntime::idle();
    let calls = runtime.set_model_calls.clone();
    let mut app = ChatApp::new_with_runtime_for_test(
        env!("CARGO_MANIFEST_DIR"),
        Box::new(runtime),
        "echo".to_owned(),
    )
    .expect("app should initialize");
    app.set_model("deepseek-v4-pro");
    assert_eq!(calls.lock().unwrap().as_slice(), &["deepseek-v4-pro"]);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:  
`cargo test --lib chat_app_cancel_turn_calls_runtime_port_cancel`  
`cargo test --lib chat_app_set_model_calls_runtime_port_set_model`  
Expected: FAIL before fake runtime and helper wiring are complete.

- [ ] **Step 3: Implement fake runtime test double**

```rust
struct FakeRuntime {
    events: std::sync::Mutex<std::collections::VecDeque<crate::agent::protocol::AgentEvent>>,
    cancel_called: std::sync::Arc<std::sync::atomic::AtomicBool>,
    set_model_calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl crate::chat::runtime_port::AgentRuntimePort for FakeRuntime {
    fn submit_turn(&self, _prompt: &str) -> Result<(), String> { Ok(()) }
    fn set_model(&self, model: &str) -> Result<(), String> {
        self.set_model_calls.lock().unwrap().push(model.to_owned());
        Ok(())
    }
    fn refresh_acp_agents(&self) -> Result<(), String> { Ok(()) }
    fn cancel(&self) { self.cancel_called.store(true, std::sync::atomic::Ordering::SeqCst); }
    fn try_recv(&self) -> Option<crate::agent::protocol::AgentEvent> {
        self.events.lock().unwrap().pop_front()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:  
`cargo test --lib chat_app_cancel_turn_calls_runtime_port_cancel`  
`cargo test --lib chat_app_set_model_calls_runtime_port_set_model`  
`cargo test --lib chat_app_tick_drains_events_from_runtime_port`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/unit/chat/app/tests_impl.inc src/chat/app.rs
git commit -m "test(chat): add runtime port boundary behavior tests"
```

### Task 4: Regression validation and cleanup

**Files:**
- Modify: none expected (only fixes if failures found)
- Test: existing suites

- [ ] **Step 1: Run focused chat app regression tests**

Run:
`cargo test --lib status_label_transitions_across_action_states`
`cargo test --lib follow_up_turn_keeps_action_first_status_label`
`cargo test --lib event_handlers_append_to_existing_thinking_and_response_entries`
Expected: PASS.

- [ ] **Step 2: Run repository quality gates**

Run:
`just fmt-check`
`just lint`
`just test`
Expected: PASS.

- [ ] **Step 3: Commit final integration**

```bash
git add src/chat/runtime_port.rs src/chat/mod.rs src/agent/runtime.rs src/chat/app.rs tests/unit/chat/app/tests_impl.inc
git commit -m "refactor(chat): invert runtime dependency via AgentRuntimePort"
```

## Self-review checklist

### Spec coverage

- Runtime boundary trait introduced: covered by Task 1.
- `ChatApp` depends on abstraction: covered by Task 2.
- Behavior preservation and event flow parity: covered by Tasks 3-4.
- Focused test coverage for boundary: covered by Task 3.

### Placeholder scan

- No TBD/TODO placeholders.
- Every code-changing step includes concrete code blocks.
- Every verification step includes explicit command + expected result.

### Type consistency

- Trait name is consistently `AgentRuntimePort`.
- `ChatApp` storage type consistently `Box<dyn AgentRuntimePort>`.
- Event type consistently `crate::agent::protocol::AgentEvent`.
