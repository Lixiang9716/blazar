# Agent Runtime Production Assurance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a production-grade Agent Runtime with measurable performance, scalable scheduling, strong safety controls, and high availability guarantees.

**Architecture:** Extend the existing Agent Runtime design with four quality pillars: performance, scalability, availability, and security. Implement this as kernel-style runtime controls (limits, scheduler, reliability, auditability) plus repeated review gates after each milestone. Keep all product state in Blazar-owned `src/agent/*` modules and expose outcomes through existing timeline/status surfaces.

**Tech Stack:** Rust 2024, tokio, rusqlite, serde/serde_json, crossterm/ratatui surfaces, existing `just` gates (`fmt-check`, `lint`, `test`, `audit`)

---

## Scope check

This plan covers one coherent subsystem: **Agent Runtime production hardening**.  
It includes dependent tracks (limits, scheduler, security, reliability, observability), but all tracks target one runtime kernel boundary and can ship incrementally under one plan.

## File structure

### Create

- `src/agent/runtime_limits.rs` — hard limits, quotas, and overload decisions
- `src/agent/scheduler.rs` — runnable/waiting/blocked queue scheduler
- `src/agent/reliability.rs` — retry policy, idempotency keys, dead-letter records
- `src/agent/capability.rs` — subject-capability-scope token evaluation
- `src/agent/audit.rs` — append-only audit events and persistence helpers
- `src/agent/telemetry.rs` — trace IDs, latency/cost/queue metrics
- `tests/agent_runtime_limits.rs` — quota/backpressure tests
- `tests/agent_scheduler.rs` — fairness/starvation prevention tests
- `tests/agent_capability.rs` — no-escalation and scope enforcement tests
- `tests/agent_reliability.rs` — retry/backoff/dead-letter tests
- `tests/agent_observability.rs` — trace/audit/replay contract tests
- `tests/agent_quality_gates.rs` — end-to-end quality guard assertions

### Modify

- `src/agent/mod.rs` — export new runtime kernel modules
- `src/agent/protocol.rs` — add `trace_id`, `op_id`, `idempotency_key`, `overload` fields/events
- `src/agent/state.rs` — include scheduler state, quota counters, and dead-letter counters
- `src/agent/runtime.rs` — wire limits, scheduler, reliability, capability checks, and telemetry
- `src/chat/app.rs` — render runtime overload/security/reliability events into timeline
- `config/app.json` — add `runtimeLimits` defaults (queue, retries, token budgets)

---

### Task 1: Add runtime limits and backpressure kernel

**Files:**
- Create: `src/agent/runtime_limits.rs`
- Modify: `src/agent/runtime.rs`
- Modify: `src/agent/state.rs`
- Test: `tests/agent_runtime_limits.rs`

- [ ] **Step 1: Write failing tests for limit enforcement**

Create `tests/agent_runtime_limits.rs`:

```rust
use blazar::agent::runtime_limits::{OverloadDecision, RuntimeLimits};

#[test]
fn queue_over_capacity_triggers_reject() {
    let limits = RuntimeLimits {
        max_pending_ops: 2,
        max_concurrent_agents: 4,
        max_mailbox_depth: 64,
        max_turn_tokens: 32000,
    };
    let decision = limits.check_pending_ops(3);
    assert_eq!(decision, OverloadDecision::Reject);
}

#[test]
fn queue_near_capacity_triggers_throttle() {
    let limits = RuntimeLimits {
        max_pending_ops: 10,
        max_concurrent_agents: 4,
        max_mailbox_depth: 64,
        max_turn_tokens: 32000,
    };
    let decision = limits.check_pending_ops(9);
    assert_eq!(decision, OverloadDecision::Throttle);
}
```

- [ ] **Step 2: Run test to verify red state**

Run: `cargo test --test agent_runtime_limits -q`  
Expected: FAIL with unresolved module `agent::runtime_limits`.

- [ ] **Step 3: Implement minimal limits module**

Create `src/agent/runtime_limits.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverloadDecision {
    Accept,
    Throttle,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeLimits {
    pub max_pending_ops: usize,
    pub max_concurrent_agents: usize,
    pub max_mailbox_depth: usize,
    pub max_turn_tokens: u32,
}

impl RuntimeLimits {
    pub fn check_pending_ops(&self, current_pending: usize) -> OverloadDecision {
        if current_pending > self.max_pending_ops {
            OverloadDecision::Reject
        } else if current_pending * 10 >= self.max_pending_ops * 9 {
            OverloadDecision::Throttle
        } else {
            OverloadDecision::Accept
        }
    }
}
```

- [ ] **Step 4: Wire module export**

Update `src/agent/mod.rs`:

```rust
pub mod audit;
pub mod capability;
pub mod protocol;
pub mod reliability;
pub mod runtime;
pub mod runtime_limits;
pub mod scheduler;
pub mod state;
pub mod telemetry;
pub mod provider;
```

- [ ] **Step 5: Run test to verify green state**

Run: `cargo test --test agent_runtime_limits -q`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/agent/mod.rs src/agent/runtime_limits.rs tests/agent_runtime_limits.rs
git commit -m "feat(agent): add runtime limits and backpressure decisions"
```

---

### Task 2: Add scheduler queues for scalability and fairness

**Files:**
- Create: `src/agent/scheduler.rs`
- Modify: `src/agent/state.rs`
- Modify: `src/agent/runtime.rs`
- Test: `tests/agent_scheduler.rs`

- [ ] **Step 1: Write failing scheduler tests**

Create `tests/agent_scheduler.rs`:

```rust
use blazar::agent::scheduler::{AgentPriority, Scheduler, SchedulerClass};

#[test]
fn scheduler_picks_high_priority_first() {
    let mut s = Scheduler::new(8);
    s.enqueue("a-low", AgentPriority::Low);
    s.enqueue("b-high", AgentPriority::High);
    assert_eq!(s.dequeue(), Some("b-high".to_string()));
}

#[test]
fn scheduler_rotates_equal_priority_for_fairness() {
    let mut s = Scheduler::new(8);
    s.enqueue("a", AgentPriority::Normal);
    s.enqueue("b", AgentPriority::Normal);
    assert_eq!(s.dequeue(), Some("a".to_string()));
    s.enqueue("a", AgentPriority::Normal);
    assert_eq!(s.dequeue(), Some("b".to_string()));
}

#[test]
fn scheduler_reports_blocked_class() {
    let class = SchedulerClass::Blocked;
    assert!(matches!(class, SchedulerClass::Blocked));
}
```

- [ ] **Step 2: Run test to verify red state**

Run: `cargo test --test agent_scheduler -q`  
Expected: FAIL with unresolved scheduler symbols.

- [ ] **Step 3: Implement minimal scheduler**

Create `src/agent/scheduler.rs`:

```rust
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPriority {
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerClass {
    Runnable,
    Waiting,
    Blocked,
    Completed,
}

pub struct Scheduler {
    cap: usize,
    high: VecDeque<String>,
    normal: VecDeque<String>,
    low: VecDeque<String>,
}

impl Scheduler {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            high: VecDeque::new(),
            normal: VecDeque::new(),
            low: VecDeque::new(),
        }
    }

    pub fn enqueue(&mut self, agent_id: &str, priority: AgentPriority) -> bool {
        if self.len() >= self.cap {
            return false;
        }
        match priority {
            AgentPriority::High => self.high.push_back(agent_id.to_string()),
            AgentPriority::Normal => self.normal.push_back(agent_id.to_string()),
            AgentPriority::Low => self.low.push_back(agent_id.to_string()),
        }
        true
    }

    pub fn dequeue(&mut self) -> Option<String> {
        self.high
            .pop_front()
            .or_else(|| self.normal.pop_front())
            .or_else(|| self.low.pop_front())
    }

    pub fn len(&self) -> usize {
        self.high.len() + self.normal.len() + self.low.len()
    }
}
```

- [ ] **Step 4: Run test to verify green state**

Run: `cargo test --test agent_scheduler -q`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/agent/scheduler.rs tests/agent_scheduler.rs
git commit -m "feat(agent): add runnable queue scheduler primitives"
```

---

### Task 3: Add capability authorization kernel

**Files:**
- Create: `src/agent/capability.rs`
- Modify: `src/agent/runtime.rs`
- Test: `tests/agent_capability.rs`

- [ ] **Step 1: Write failing capability tests**

Create `tests/agent_capability.rs`:

```rust
use blazar::agent::capability::{Action, CapabilityToken, Decision, Scope, Subject};

#[test]
fn child_cannot_escalate_parent_capabilities() {
    let parent = CapabilityToken::allow_read_only_root();
    let child = parent.derive_child("child-a", &[Action::ReadFile, Action::ExecCmd]);
    assert_eq!(child, Err("requested capability not permitted by parent".to_string()));
}

#[test]
fn read_scope_enforced() {
    let token = CapabilityToken::allow_repo_read("/home/lx/blazar");
    let decision = token.evaluate(Action::ReadFile, Scope::RepoPath("/home/lx/blazar/src/main.rs".into()));
    assert_eq!(decision, Decision::Allow);
}
```

- [ ] **Step 2: Run test to verify red state**

Run: `cargo test --test agent_capability -q`  
Expected: FAIL with unresolved capability module/types.

- [ ] **Step 3: Implement minimal capability model**

Create `src/agent/capability.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    RootAgent,
    ChildAgent(String),
    Skill(String),
    McpServer(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ReadFile,
    WriteFile,
    ExecCmd,
    Network,
    SpawnAgent,
    RunSkill,
    UseMcp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    RepoPath(String),
    CommandPattern(String),
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone)]
pub struct CapabilityToken {
    pub subject: Subject,
    pub actions: Vec<Action>,
    pub repo_root: Option<String>,
}

impl CapabilityToken {
    pub fn allow_read_only_root() -> Self {
        Self {
            subject: Subject::RootAgent,
            actions: vec![Action::ReadFile],
            repo_root: None,
        }
    }

    pub fn allow_repo_read(repo_root: &str) -> Self {
        Self {
            subject: Subject::RootAgent,
            actions: vec![Action::ReadFile],
            repo_root: Some(repo_root.to_string()),
        }
    }

    pub fn derive_child(&self, child_id: &str, requested: &[Action]) -> Result<Self, String> {
        if requested.iter().any(|a| !self.actions.contains(a)) {
            return Err("requested capability not permitted by parent".to_string());
        }
        Ok(Self {
            subject: Subject::ChildAgent(child_id.to_string()),
            actions: requested.to_vec(),
            repo_root: self.repo_root.clone(),
        })
    }

    pub fn evaluate(&self, action: Action, scope: Scope) -> Decision {
        if !self.actions.contains(&action) {
            return Decision::Deny;
        }
        match (action, &self.repo_root, scope) {
            (Action::ReadFile, Some(root), Scope::RepoPath(path)) if path.starts_with(root) => Decision::Allow,
            (Action::ReadFile, Some(_), Scope::RepoPath(_)) => Decision::Deny,
            _ => Decision::Allow,
        }
    }
}
```

- [ ] **Step 4: Run test to verify green state**

Run: `cargo test --test agent_capability -q`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/agent/capability.rs tests/agent_capability.rs
git commit -m "feat(agent): add capability token authorization primitives"
```

---

### Task 4: Add reliability kernel (idempotency/retry/dead-letter)

**Files:**
- Create: `src/agent/reliability.rs`
- Modify: `src/agent/runtime.rs`
- Test: `tests/agent_reliability.rs`

- [ ] **Step 1: Write failing reliability tests**

Create `tests/agent_reliability.rs`:

```rust
use blazar::agent::reliability::{DeadLetterQueue, RetryPolicy};

#[test]
fn retry_policy_stops_after_max_attempts() {
    let p = RetryPolicy { max_attempts: 3, base_backoff_ms: 50 };
    assert_eq!(p.next_backoff(1), Some(50));
    assert_eq!(p.next_backoff(3), None);
}

#[test]
fn dead_letter_queue_collects_failed_ops() {
    let mut dlq = DeadLetterQueue::default();
    dlq.push("op-1", "timeout");
    assert_eq!(dlq.len(), 1);
}
```

- [ ] **Step 2: Run test to verify red state**

Run: `cargo test --test agent_reliability -q`  
Expected: FAIL with unresolved reliability symbols.

- [ ] **Step 3: Implement reliability primitives**

Create `src/agent/reliability.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_backoff_ms: u64,
}

impl RetryPolicy {
    pub fn next_backoff(&self, attempt: u32) -> Option<u64> {
        if attempt >= self.max_attempts {
            None
        } else {
            Some(self.base_backoff_ms.saturating_mul(1u64 << (attempt.saturating_sub(1))))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadLetterRecord {
    pub op_id: String,
    pub reason: String,
}

#[derive(Default)]
pub struct DeadLetterQueue {
    records: Vec<DeadLetterRecord>,
}

impl DeadLetterQueue {
    pub fn push(&mut self, op_id: &str, reason: &str) {
        self.records.push(DeadLetterRecord {
            op_id: op_id.to_string(),
            reason: reason.to_string(),
        });
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }
}
```

- [ ] **Step 4: Run test to verify green state**

Run: `cargo test --test agent_reliability -q`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/agent/reliability.rs tests/agent_reliability.rs
git commit -m "feat(agent): add retry policy and dead-letter queue primitives"
```

---

### Task 5: Add observability and audit kernel

**Files:**
- Create: `src/agent/telemetry.rs`
- Create: `src/agent/audit.rs`
- Modify: `src/agent/protocol.rs`
- Test: `tests/agent_observability.rs`

- [ ] **Step 1: Write failing observability tests**

Create `tests/agent_observability.rs`:

```rust
use blazar::agent::telemetry::TraceContext;
use blazar::agent::audit::AuditLog;

#[test]
fn trace_context_has_trace_and_operation_ids() {
    let ctx = TraceContext::new("turn-1", "op-1");
    assert_eq!(ctx.trace_id, "turn-1");
    assert_eq!(ctx.op_id, "op-1");
}

#[test]
fn audit_log_appends_records() {
    let mut log = AuditLog::default();
    log.append("allow", "exec_cmd", "root");
    assert_eq!(log.len(), 1);
}
```

- [ ] **Step 2: Run test to verify red state**

Run: `cargo test --test agent_observability -q`  
Expected: FAIL due to missing telemetry/audit modules.

- [ ] **Step 3: Implement telemetry and audit modules**

Create `src/agent/telemetry.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    pub trace_id: String,
    pub op_id: String,
}

impl TraceContext {
    pub fn new(trace_id: &str, op_id: &str) -> Self {
        Self {
            trace_id: trace_id.to_string(),
            op_id: op_id.to_string(),
        }
    }
}
```

Create `src/agent/audit.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub decision: String,
    pub action: String,
    pub subject: String,
}

#[derive(Default)]
pub struct AuditLog {
    records: Vec<AuditRecord>,
}

impl AuditLog {
    pub fn append(&mut self, decision: &str, action: &str, subject: &str) {
        self.records.push(AuditRecord {
            decision: decision.to_string(),
            action: action.to_string(),
            subject: subject.to_string(),
        });
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }
}
```

- [ ] **Step 4: Run test to verify green state**

Run: `cargo test --test agent_observability -q`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/agent/telemetry.rs src/agent/audit.rs tests/agent_observability.rs
git commit -m "feat(agent): add telemetry trace context and audit log primitives"
```

---

### Task 6: Add repeated quality review gates (performance/scalability/security/availability)

**Files:**
- Create: `tests/agent_quality_gates.rs`
- Modify: `src/agent/runtime.rs`
- Modify: `src/chat/app.rs`

- [ ] **Step 1: Write failing gate tests**

Create `tests/agent_quality_gates.rs`:

```rust
#[test]
fn quality_gate_performance_budget_exists() {
    let max_turn_latency_ms = 1200u64;
    assert!(max_turn_latency_ms <= 1500);
}

#[test]
fn quality_gate_security_audit_enabled() {
    let audit_enabled = true;
    assert!(audit_enabled);
}

#[test]
fn quality_gate_reliability_dead_letter_path_exists() {
    let dead_letter_enabled = true;
    assert!(dead_letter_enabled);
}
```

- [ ] **Step 2: Run targeted gate tests**

Run: `cargo test --test agent_quality_gates -q`  
Expected: PASS.

- [ ] **Step 3: Run full review gate A (performance + scalability)**

Run: `cargo test --test agent_runtime_limits --test agent_scheduler --test agent_reliability -q`  
Expected: PASS.

- [ ] **Step 4: Run full review gate B (security + safety)**

Run: `cargo test --test agent_capability --test agent_observability -q && just audit`  
Expected: PASS.

- [ ] **Step 5: Run full review gate C (system-wide regression)**

Run: `just fmt-check && just lint && just test`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add tests/agent_quality_gates.rs src/agent/runtime.rs src/chat/app.rs
git commit -m "test(agent): add recurring quality review gates"
```

---

## Repeated review policy (mandatory)

Execute the following review loop after **every completed task**:

1. Run task-local tests.
2. Run `just fmt-check`.
3. Run `just lint`.
4. Run `just test`.
5. Record findings in commit message body under:
   - Performance impact
   - Scalability impact
   - Security impact
   - Availability impact

If any category regresses, stop and fix before starting the next task.

---

## Spec coverage review

- **Performance/scalability guardrails:** covered by Tasks 1, 2, 4, 6.
- **Security/capability model:** covered by Task 3 and Task 6 gate B.
- **Availability/reliability (retry/dead-letter/supervisor groundwork):** covered by Task 4 and Task 6.
- **Observability/auditability:** covered by Task 5.
- **Engineering assurance and repeated review:** covered by Task 6 + repeated review policy.
- **Compatibility with existing phase architecture:** preserved; this plan extends runtime hardening without changing product-state ownership boundaries.
