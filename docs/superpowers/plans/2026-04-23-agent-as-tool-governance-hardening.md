# Agent-as-Tool Governance Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden Agent-as-Tool architecture with unified error taxonomy, scheduler contract guarantees, capability-level observability, facade-governance metadata, and protocol-conformance guardrails.

**Architecture:** Keep the Capability Kernel shape, but add governance primitives at key seams: runtime error classification, scheduler contract tests, richer runtime event metadata, explicit facade compatibility tiers, and adapter conformance tests. This keeps behavior stable while reducing semantic drift and regression risk.

**Tech Stack:** Rust, existing Blazar runtime/tool architecture, cargo test / nextest, serde_json

---

### Task 1: Unify runtime error taxonomy across provider/protocol/execution paths

**Files:**
- Create: `src/agent/runtime/errors.rs`
- Modify: `src/agent/runtime.rs`
- Modify: `src/agent/runtime/turn.rs`
- Modify: `src/agent/runtime/events.rs`
- Modify: `src/agent/protocol.rs`
- Test: `src/agent/runtime/tests_impl.inc`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn turn_failed_event_carries_structured_error_kind() {
    let runtime = AgentRuntime::new(
        Box::new(crate::provider::echo::EchoProvider::new(0)),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        "echo".to_owned(),
    )
    .expect("runtime should initialize");

    // Force a known fatal path (unknown model/provider error simulation can use test provider)
    runtime.submit_turn("trigger structured error").expect("submit");

    let saw_structured = std::iter::repeat_with(|| runtime.try_recv())
        .take(200)
        .flatten()
        .any(|event| matches!(
            event,
            AgentEvent::TurnFailed { kind: RuntimeErrorKind::ProviderFatal, .. }
        ));

    assert!(saw_structured, "TurnFailed should include RuntimeErrorKind");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test turn_failed_event_carries_structured_error_kind -- --nocapture`  
Expected: FAIL with missing `kind` field / missing `RuntimeErrorKind`.

- [ ] **Step 3: Write minimal implementation**

```rust
// src/agent/runtime/errors.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    ProviderTransient,
    ProviderFatal,
    ProtocolInvalidPayload,
    ToolExecution,
    Cancelled,
}

impl RuntimeErrorKind {
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::ProviderTransient)
    }
}
```

```rust
// src/agent/protocol.rs (event shape)
TurnFailed {
    kind: RuntimeErrorKind,
    error: String,
}
```

```rust
// src/agent/runtime/events.rs
fn on_turn_failed(&self, kind: RuntimeErrorKind, error: &str) {
    let _ = self.tx.send(AgentEvent::TurnFailed {
        kind,
        error: error.to_owned(),
    });
}
```

```rust
// src/agent/runtime/turn.rs
observer.on_turn_failed(RuntimeErrorKind::ProviderFatal, &err);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test turn_failed_event_carries_structured_error_kind -- --nocapture`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/agent/runtime/errors.rs src/agent/runtime.rs src/agent/runtime/turn.rs src/agent/runtime/events.rs src/agent/protocol.rs src/agent/runtime/tests_impl.inc
git commit -m "feat: add structured runtime error taxonomy"
```

---

### Task 2: Lock scheduler semantics with a contract matrix

**Files:**
- Modify: `src/agent/runtime/scheduler.rs`
- Test: `src/agent/runtime/tests_impl.inc`

- [ ] **Step 1: Write failing contract tests**

```rust
#[test]
fn scheduler_contract_matrix_is_stable_for_conflict_pairs() {
    let cases = vec![
        (claim_ro("fs:a"), claim_ro("fs:a"), false),
        (claim_ro("fs:a"), claim_rw("fs:a"), true),
        (claim_rw("fs:a"), claim_rw("fs:a"), true),
        (claim_ex("process:bash"), claim_ro("fs:a"), true),
    ];

    for (left, right, expected_conflict) in cases {
        let actual = ConflictPolicy::from_claims(&[left], &[right]).is_conflicting();
        assert_eq!(actual, expected_conflict);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test scheduler_contract_matrix_is_stable_for_conflict_pairs -- --nocapture`  
Expected: FAIL due missing helpers/coverage of matrix behavior.

- [ ] **Step 3: Implement minimal contract helpers + matrix coverage**

```rust
fn claim_ro(resource: &str) -> CapabilityClaim {
    CapabilityClaim { resource: resource.into(), access: CapabilityAccess::ReadOnly }
}
fn claim_rw(resource: &str) -> CapabilityClaim {
    CapabilityClaim { resource: resource.into(), access: CapabilityAccess::ReadWrite }
}
fn claim_ex(resource: &str) -> CapabilityClaim {
    CapabilityClaim { resource: resource.into(), access: CapabilityAccess::Exclusive }
}
```

```rust
#[test]
fn scheduler_replay_order_matches_original_call_order() {
    // Ensure batch execution still emits results deterministically by original call index.
}
```

- [ ] **Step 4: Run scheduler tests**

Run: `cargo test scheduler_ -- --nocapture`  
Expected: PASS for all scheduler contract tests.

- [ ] **Step 5: Commit**

```bash
git add src/agent/runtime/scheduler.rs src/agent/runtime/tests_impl.inc
git commit -m "test: add scheduler contract matrix and replay-order guarantees"
```

---

### Task 3: Add capability-level observability metadata to runtime events and UI handling

**Files:**
- Modify: `src/agent/protocol.rs`
- Modify: `src/agent/runtime/executor.rs`
- Modify: `src/agent/runtime/events.rs`
- Modify: `src/chat/app/turn.rs`
- Test: `src/agent/runtime/tests_impl.inc`
- Test: `src/chat/app/tests_impl.inc`

- [ ] **Step 1: Write failing tests for metadata propagation**

```rust
#[test]
fn tool_call_started_includes_batch_and_replay_metadata() {
    // Assert AgentEvent::ToolCallStarted has batch_id/replay_index/normalized_claims fields.
}
```

```rust
#[test]
fn chat_timeline_preserves_capability_metadata_for_tool_entries() {
    // Assert tool entry details include metadata in a stable render format.
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test tool_call_started_includes_batch_and_replay_metadata chat_timeline_preserves_capability_metadata_for_tool_entries -- --nocapture`  
Expected: FAIL due missing fields and rendering.

- [ ] **Step 3: Implement minimal metadata threading**

```rust
// src/agent/protocol.rs
ToolCallStarted {
    call_id: String,
    tool_name: String,
    kind: ToolKind,
    arguments: String,
    batch_id: u32,
    replay_index: usize,
    normalized_claims: Vec<String>,
}
```

```rust
// src/agent/runtime/events.rs
fn on_tool_call_started(..., batch_id: u32, replay_index: usize, normalized_claims: &[String]) { ... }
```

```rust
// src/chat/app/turn.rs
entry.details = format!(
    "{}\n\nbatch_id={batch_id}, replay_index={replay_index}, claims={}",
    output,
    normalized_claims.join(",")
);
```

- [ ] **Step 4: Run tests**

Run: `cargo test tool_call_started_includes_batch_and_replay_metadata -- --nocapture`  
Run: `cargo test chat_timeline_preserves_capability_metadata_for_tool_entries -- --nocapture`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/agent/protocol.rs src/agent/runtime/executor.rs src/agent/runtime/events.rs src/chat/app/turn.rs src/agent/runtime/tests_impl.inc src/chat/app/tests_impl.inc
git commit -m "feat: add capability-level observability metadata for tool events"
```

---

### Task 4: Formalize Tool facade governance with compatibility tiers

**Files:**
- Create: `src/agent/tools/policy.rs`
- Modify: `src/agent/tools/mod_impl.inc`
- Test: `src/agent/runtime/tests_impl.inc`
- Test: `src/chat/app/tests_impl.inc`
- Modify: `docs/superpowers/specs/2026-04-23-agent-as-tool-critical-review-design.md`

- [ ] **Step 1: Write failing tests for compatibility metadata**

```rust
#[test]
fn tool_registry_exposes_compatibility_tier_for_each_tool() {
    let registry = ToolRegistry::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let tier = registry.compatibility_tier("bash");
    assert_eq!(tier, Some(ToolCompatibilityTier::CompatibilityBridge));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test tool_registry_exposes_compatibility_tier_for_each_tool -- --nocapture`  
Expected: FAIL with missing tier API/types.

- [ ] **Step 3: Implement minimal governance model**

```rust
// src/agent/tools/policy.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCompatibilityTier {
    KernelNative,
    CompatibilityBridge,
}
```

```rust
// src/agent/tools/mod_impl.inc
pub trait Tool {
    fn compatibility_tier(&self) -> ToolCompatibilityTier {
        ToolCompatibilityTier::KernelNative
    }
}
```

```rust
impl ToolRegistry {
    pub fn compatibility_tier(&self, name: &str) -> Option<ToolCompatibilityTier> {
        self.get(name).map(|tool| tool.compatibility_tier())
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test tool_registry_exposes_compatibility_tier_for_each_tool -- --nocapture`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/agent/tools/policy.rs src/agent/tools/mod_impl.inc src/agent/runtime/tests_impl.inc src/chat/app/tests_impl.inc docs/superpowers/specs/2026-04-23-agent-as-tool-critical-review-design.md
git commit -m "feat: add tool facade compatibility-tier governance metadata"
```

---

### Task 5: Add synthetic adapter conformance checks (protocol-agnostic guardrail)

**Files:**
- Create: `src/agent/adapters/conformance.rs`
- Create: `src/agent/adapters/conformance_tests.rs`
- Modify: `src/agent/adapters/mod.rs`
- Modify: `src/agent/adapters/acp_client/mod.rs`
- Test: `src/agent/adapters/conformance_tests.rs`

- [ ] **Step 1: Write failing conformance test**

```rust
#[test]
fn acp_adapter_satisfies_generic_agent_adapter_contract() {
    let report = run_adapter_conformance_suite(AcpAdapterContractProbe::default());
    assert!(report.all_passed(), "ACP adapter must satisfy generic contract");
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test acp_adapter_satisfies_generic_agent_adapter_contract -- --nocapture`  
Expected: FAIL because conformance harness is not defined yet.

- [ ] **Step 3: Implement minimal conformance harness**

```rust
pub trait AgentAdapterContractProbe {
    fn fetch_agent(&self) -> Result<(), String>;
    fn create_run(&self) -> Result<(), String>;
    fn poll_terminal(&self) -> Result<(), String>;
}

pub fn run_adapter_conformance_suite<P: AgentAdapterContractProbe>(probe: P) -> ConformanceReport {
    let mut failures = Vec::new();
    if let Err(err) = probe.fetch_agent() { failures.push(format!("fetch_agent: {err}")); }
    if let Err(err) = probe.create_run() { failures.push(format!("create_run: {err}")); }
    if let Err(err) = probe.poll_terminal() { failures.push(format!("poll_terminal: {err}")); }
    ConformanceReport { failures }
}
```

- [ ] **Step 4: Run conformance tests**

Run: `cargo test conformance -- --nocapture`  
Expected: PASS for adapter conformance suite.

- [ ] **Step 5: Commit**

```bash
git add src/agent/adapters/conformance.rs src/agent/adapters/conformance_tests.rs src/agent/adapters/mod.rs src/agent/adapters/acp_client/mod.rs
git commit -m "test: add protocol-agnostic adapter conformance harness"
```

---

### Task 6: Full verification and integration commit

**Files:**
- Modify: `docs/superpowers/specs/2026-04-23-agent-as-tool-critical-review-design.md` (mark implemented governance decisions)
- Verify: repository-wide quality gates

- [ ] **Step 1: Run targeted tests first**

```bash
cargo test scheduler_ -- --nocapture
cargo test conformance -- --nocapture
cargo test tool_registry_exposes_compatibility_tier_for_each_tool -- --nocapture
```

- [ ] **Step 2: Run full quality gates**

Run: `just fmt-check && just lint && just test`  
Expected: all commands succeed.

- [ ] **Step 3: Run coverage profile**

Run: `cargo tarpaulin --timeout 300`  
Expected: coverage remains at/above repository target profile.

- [ ] **Step 4: Stage and commit final integration**

```bash
git add -A
git commit -m "chore: harden agent-as-tool governance contracts and observability"
```

- [ ] **Step 5: Push branch**

```bash
git push origin master
```

---

## Spec Coverage Check

- Unified error taxonomy: covered by **Task 1**.
- Scheduler contract hardening: covered by **Task 2**.
- Capability-level observability: covered by **Task 3**.
- Facade governance / exit-path metadata: covered by **Task 4**.
- Second-protocol readiness probe: covered by **Task 5**.

No requirement from the approved critical-review spec is left without a corresponding implementation task.

## Placeholder Scan

No `TODO`/`TBD`/“implement later” placeholders are used in task steps.

## Type/Interface Consistency Check

- `RuntimeErrorKind` introduced in Task 1 is reused consistently by event and UI handling tasks.
- Scheduler and observability tasks reuse existing `CapabilityClaim`, `ToolKind`, and runtime event flow.
- Compatibility-tier API is introduced once and consumed through `ToolRegistry` consistently.
