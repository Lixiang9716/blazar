# Plan Plugin React Segments Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn `/plan` into a plugin-owned multi-segment ReAct workflow with plugin-local session/state storage and execution continuation via Enter.

**Architecture:** Keep core chat runtime semantics unchanged and move plan-specific workflow into `builtins/plan` units (`command/session/store`). Use JSON files in `.blazar/plans` as Git-tracked source of truth and `.blazar/state/plan_index.db` as local SQLite query index. Drive each segment as strict micro-steps (one decision per cycle), with explicit phase transitions stored in plan session state.

**Tech Stack:** Rust, serde/serde_json, rusqlite, inventory plugin registration, tokio async command execution

---

### Task 1: Split `/plan` into plugin folder units

**Files:**
- Modify: `src/chat/commands/builtins/plan.rs`
- Create: `src/chat/commands/builtins/plan/command.rs`
- Create: `src/chat/commands/builtins/plan/session.rs`
- Create: `src/chat/commands/builtins/plan/store.rs`
- Test: `tests/chat_command_registry.rs`

- [ ] **Step 1: Write failing registry test for `/plan` after module split**

```rust
#[test]
fn registry_still_contains_plan_command_after_module_split() {
    let registry = CommandRegistry::with_builtins().expect("registry");
    assert!(registry.find("/plan").is_some());
}
```

- [ ] **Step 2: Run test to verify baseline (before split)**

Run: `cargo test tests::chat_command_registry::registry_still_contains_plan_command_after_module_split -- --exact`  
Expected: PASS (safety check before refactor).

- [ ] **Step 3: Implement module split with plan.rs as entry**

```rust
// src/chat/commands/builtins/plan.rs
mod command;
mod session;
mod store;
```

- [ ] **Step 4: Move existing `/plan` command implementation into `plan/command.rs`**

```rust
pub(super) fn build_plan_command() -> Arc<dyn PaletteCommand> {
    Arc::new(PlanCommand {
        spec: CommandSpec {
            name: "/plan".to_owned(),
            description: "Run segmented planning workflow".to_owned(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "goal": { "type": "string" },
                    "continue_id": { "type": "string" }
                }
            }),
        },
    })
}
```

- [ ] **Step 5: Run focused tests**

Run: `cargo test --test chat_command_registry -- --nocapture`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/chat/commands/builtins/plan.rs src/chat/commands/builtins/plan/command.rs src/chat/commands/builtins/plan/session.rs src/chat/commands/builtins/plan/store.rs tests/chat_command_registry.rs
git commit -m "refactor: split /plan plugin into command session store units"
```

---

### Task 2: Implement plan session state machine (strict micro-step)

**Files:**
- Modify: `src/chat/commands/builtins/plan/session.rs`
- Test: `src/chat/commands/builtins/plan/session.rs` (unit tests)

- [ ] **Step 1: Write failing tests for phase transitions**

```rust
#[test]
fn next_phase_from_discover_moves_to_clarify_when_questions_exist() {
    let mut session = PlanSession::new("goal".into());
    session.pending_questions = vec!["Which repo?".into()];
    session.advance_after_react(ReactOutcome::NeedClarification);
    assert_eq!(session.phase, PlanPhase::Clarify);
}

#[test]
fn execute_step_always_returns_to_review_phase() {
    let mut session = PlanSession::new("goal".into());
    session.phase = PlanPhase::ExecuteStep;
    session.advance_after_react(ReactOutcome::ActionSucceeded);
    assert_eq!(session.phase, PlanPhase::Review);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test plan::session::tests -- --nocapture`  
Expected: FAIL with missing `PlanSession`/`PlanPhase` members.

- [ ] **Step 3: Add session model and transition logic**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanPhase { Discover, Clarify, DraftStep, FinalizePlan, ExecuteStep, Review, Done }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSession {
    pub id: String,
    pub goal: String,
    pub phase: PlanPhase,
    pub steps: Vec<PlanStep>,
    pub current_step: Option<usize>,
    pub pending_questions: Vec<String>,
}
```

- [ ] **Step 4: Implement micro-step transition function**

```rust
impl PlanSession {
    pub fn advance_after_react(&mut self, outcome: ReactOutcome) {
        self.phase = match (self.phase, outcome) {
            (PlanPhase::Discover, ReactOutcome::NeedClarification) => PlanPhase::Clarify,
            (PlanPhase::Discover, ReactOutcome::ReadyToDraft) => PlanPhase::DraftStep,
            (PlanPhase::ExecuteStep, _) => PlanPhase::Review,
            (PlanPhase::Review, ReactOutcome::ContinueExecution) => PlanPhase::ExecuteStep,
            (PlanPhase::Review, ReactOutcome::Done) => PlanPhase::Done,
            (phase, _) => phase,
        };
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test plan::session::tests -- --nocapture`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/chat/commands/builtins/plan/session.rs
git commit -m "feat: add /plan segmented session state machine"
```

---

### Task 3: Implement JSON source-of-truth plan storage

**Files:**
- Modify: `src/chat/commands/builtins/plan/store.rs`
- Test: `src/chat/commands/builtins/plan/store.rs` (unit tests)

- [ ] **Step 1: Write failing storage tests with temp workspace**

```rust
#[test]
fn save_and_load_plan_session_round_trip() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = PlanStore::new(temp.path().to_path_buf());
    let session = PlanSession::new("implement /plan".into());
    store.save_json(&session).expect("save");
    let loaded = store.load_json(&session.id).expect("load");
    assert_eq!(loaded.goal, session.goal);
}
```

- [ ] **Step 2: Run failing tests**

Run: `cargo test plan::store::tests::save_and_load_plan_session_round_trip -- --exact`  
Expected: FAIL (unimplemented store API).

- [ ] **Step 3: Implement JSON pathing and serde persistence**

```rust
impl PlanStore {
    pub fn plan_json_path(&self, plan_id: &str) -> PathBuf {
        self.workspace_root.join(".blazar/plans").join(format!("{plan_id}.json"))
    }

    pub fn save_json(&self, session: &PlanSession) -> Result<(), CommandError> {
        let path = self.plan_json_path(&session.id);
        std::fs::create_dir_all(path.parent().expect("plan dir"))?;
        let bytes = serde_json::to_vec_pretty(session)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
}
```

- [ ] **Step 4: Add load + list helpers**

```rust
pub fn load_json(&self, plan_id: &str) -> Result<PlanSession, CommandError> {
    let path = self.plan_json_path(plan_id);
    let bytes = std::fs::read(path).map_err(|e| CommandError::ExecutionFailed(e.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|e| CommandError::ExecutionFailed(e.to_string()))
}

pub fn list_plan_ids(&self) -> Result<Vec<String>, CommandError> {
    let dir = self.workspace_root.join(".blazar/plans");
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| CommandError::ExecutionFailed(e.to_string()))? {
        let entry = entry.map_err(|e| CommandError::ExecutionFailed(e.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                ids.push(stem.to_owned());
            }
        }
    }
    ids.sort();
    Ok(ids)
}
```

- [ ] **Step 5: Run store tests**

Run: `cargo test plan::store::tests -- --nocapture`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/chat/commands/builtins/plan/store.rs
git commit -m "feat: persist /plan sessions as git-tracked json"
```

---

### Task 4: Implement SQLite index sync and rebuild

**Files:**
- Modify: `src/chat/commands/builtins/plan/store.rs`
- Test: `src/chat/commands/builtins/plan/store.rs` (unit tests)

- [ ] **Step 1: Write failing index tests**

```rust
#[test]
fn syncing_json_populates_sqlite_index() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = PlanStore::new(temp.path().to_path_buf());
    let session = PlanSession::new("goal".into());
    store.save_json(&session).expect("save");
    store.sync_index_for_plan(&session.id).expect("sync");
    let plans = store.query_indexed_plans().expect("query");
    assert!(plans.iter().any(|p| p.id == session.id));
}
```

- [ ] **Step 2: Run failing test**

Run: `cargo test plan::store::tests::syncing_json_populates_sqlite_index -- --exact`  
Expected: FAIL with missing index APIs.

- [ ] **Step 3: Add schema bootstrap and sync methods**

```rust
fn ensure_schema(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS plans (
             id TEXT PRIMARY KEY,
             goal TEXT NOT NULL,
             status TEXT NOT NULL,
             phase TEXT NOT NULL,
             updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS plan_steps (
             plan_id TEXT NOT NULL,
             step_index INTEGER NOT NULL,
             title TEXT NOT NULL,
             status TEXT NOT NULL,
             PRIMARY KEY (plan_id, step_index)
         );
         CREATE TABLE IF NOT EXISTS plan_events (
             plan_id TEXT NOT NULL,
             seq INTEGER NOT NULL,
             event_type TEXT NOT NULL,
             summary TEXT NOT NULL,
             created_at TEXT NOT NULL,
             PRIMARY KEY (plan_id, seq)
         );",
    )
}
```

- [ ] **Step 4: Add rebuild-from-json implementation**

```rust
pub fn rebuild_index_from_json(&self) -> Result<(), CommandError> {
    let ids = self.list_plan_ids()?;
    for id in ids {
        self.sync_index_for_plan(&id)?;
    }
    Ok(())
}
```

- [ ] **Step 5: Run store test suite**

Run: `cargo test plan::store::tests -- --nocapture`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/chat/commands/builtins/plan/store.rs
git commit -m "feat: add sqlite index sync for /plan sessions"
```

---

### Task 5: Implement `/plan` command orchestration (plugin-owned)

**Files:**
- Modify: `src/chat/commands/builtins/plan/command.rs`
- Modify: `src/chat/commands/builtins/plan/session.rs`
- Modify: `src/chat/commands/builtins/plan/store.rs`
- Test: `tests/chat_command_orchestrator.rs`

- [ ] **Step 1: Write failing orchestrator tests for `/plan` lifecycle**

```rust
#[tokio::test]
async fn execute_plan_command_creates_plan_session_and_prompts_execution() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let result = execute_command_for_test(&mut app, "/plan", json!({"goal":"cleanup"})).await.expect("plan");
    assert!(result.summary.contains("Plan drafted"));
    assert!(app.timeline().iter().any(|e| e.body.contains("Press Enter to execute")));
}
```

- [ ] **Step 2: Run failing orchestrator test**

Run: `cargo test --test chat_command_orchestrator execute_plan_command_creates_plan_session_and_prompts_execution -- --exact`  
Expected: FAIL (current `/plan` only pre-fills composer).

- [ ] **Step 3: Implement command argument parsing and session bootstrap**

```rust
#[derive(Deserialize)]
struct PlanArgs { goal: Option<String>, continue_id: Option<String> }

fn bootstrap_plan_session(goal: &str, store: &PlanStore) -> Result<PlanSession, CommandError> {
    let mut session = PlanSession::new(goal.to_owned());
    session.phase = PlanPhase::Discover;
    store.save_json(&session)?;
    Ok(session)
}
```

- [ ] **Step 4: Implement phase-runner entry for one micro-step**

```rust
fn run_one_segment(session: &mut PlanSession, app: &mut ChatApp) -> Result<SegmentResult, CommandError> {
    let result = match session.phase {
        PlanPhase::Discover => SegmentResult::NeedClarification,
        PlanPhase::Clarify => SegmentResult::NeedClarification,
        PlanPhase::DraftStep => SegmentResult::PlanStepDrafted,
        PlanPhase::FinalizePlan => SegmentResult::ReadyToExecute,
        PlanPhase::ExecuteStep => SegmentResult::ActionExecuted,
        PlanPhase::Review => SegmentResult::ContinueExecution,
        PlanPhase::Done => SegmentResult::Done,
    };
    app.push_system_hint(format!("plan phase {:?} -> {:?}", session.phase, result));
    Ok(result)
}
```

- [ ] **Step 5: Emit timeline guidance and Enter continuation hint**

```rust
app.push_system_hint("Plan drafted. Press Enter to execute the next step.");
app.set_composer_text(&format!("/plan --continue {}", session.id));
```

- [ ] **Step 6: Run orchestrator tests**

Run: `cargo test --test chat_command_orchestrator execute_plan_command -- --nocapture`  
Expected: PASS for `/plan` behavior assertions.

- [ ] **Step 7: Commit**

```bash
git add src/chat/commands/builtins/plan/command.rs src/chat/commands/builtins/plan/session.rs src/chat/commands/builtins/plan/store.rs tests/chat_command_orchestrator.rs
git commit -m "feat: implement /plan multi-segment plugin workflow"
```

---

### Task 6: Add clarify flow (`ask_user`) and execution review loop

**Files:**
- Modify: `src/chat/commands/builtins/plan/command.rs`
- Modify: `src/chat/commands/builtins/plan/session.rs`
- Test: `tests/chat_command_orchestrator.rs`

- [ ] **Step 1: Write failing tests for clarify and review branches**

```rust
#[tokio::test]
async fn plan_command_routes_to_clarify_when_goal_is_ambiguous() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let result = execute_command_for_test(&mut app, "/plan", json!({"goal":"fix it"})).await.expect("plan");
    assert!(result.summary.contains("clarification required"));
}
```

- [ ] **Step 2: Run failing test**

Run: `cargo test --test chat_command_orchestrator plan_command_routes_to_clarify_when_goal_is_ambiguous -- --exact`  
Expected: FAIL.

- [ ] **Step 3: Implement clarify branch in phase runner**

```rust
if session.phase == PlanPhase::Clarify {
    app.push_system_hint("Need clarification before drafting full plan.");
    return Ok(SegmentResult::NeedsClarification);
}
```

- [ ] **Step 4: Implement review decisions after each execution micro-step**

```rust
match review_outcome {
    ReviewOutcome::Continue => session.phase = PlanPhase::ExecuteStep,
    ReviewOutcome::Revise => session.phase = PlanPhase::DraftStep,
    ReviewOutcome::Fail => session.phase = PlanPhase::Done,
}
```

- [ ] **Step 5: Run command tests**

Run: `cargo test --test chat_command_orchestrator execute_plan_command -- --nocapture`  
Expected: PASS including clarify/review coverage.

- [ ] **Step 6: Commit**

```bash
git add src/chat/commands/builtins/plan/command.rs src/chat/commands/builtins/plan/session.rs tests/chat_command_orchestrator.rs
git commit -m "feat: add clarify and review loops to /plan segmented execution"
```

---

### Task 7: Integration and persistence recovery tests

**Files:**
- Create: `tests/chat_plan_plugin.rs`
- Modify: `src/chat/commands/builtins/plan/store.rs`

- [ ] **Step 1: Write end-to-end flow test**

```rust
#[tokio::test]
async fn plan_plugin_end_to_end_reaches_done() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let mut app = ChatApp::new_for_test(workspace.path().to_str().expect("workspace path"))
        .expect("app");
    execute_command_for_test(&mut app, "/plan", json!({"goal":"add tests"}))
        .await
        .expect("plan start");
    let continue_id = extract_continue_id_from_composer(&app.composer_text());
    for _ in 0..6 {
        execute_command_for_test(
            &mut app,
            "/plan",
            json!({"continue_id": continue_id.clone()}),
        )
        .await
        .expect("continue");
    }
    let store = PlanStore::new(workspace.path().to_path_buf());
    let loaded = store.load_json(&continue_id).expect("load plan");
    assert_eq!(loaded.phase, PlanPhase::Done);
}
```

- [ ] **Step 2: Write index rebuild test**

```rust
#[tokio::test]
async fn missing_index_rebuilds_from_json_files() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let store = PlanStore::new(workspace.path().to_path_buf());
    let plan = PlanSession::new("rebuild index".to_owned());
    store.save_json(&plan).expect("save json");
    let db_path = workspace.path().join(".blazar/state/plan_index.db");
    if db_path.exists() {
        std::fs::remove_file(&db_path).expect("remove old db");
    }
    store.rebuild_index_from_json().expect("rebuild");
    let rows = store.query_indexed_plans().expect("query");
    assert!(rows.iter().any(|row| row.id == plan.id));
}
```

- [ ] **Step 3: Run integration tests**

Run: `cargo test --test chat_plan_plugin -- --nocapture`  
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add tests/chat_plan_plugin.rs src/chat/commands/builtins/plan/store.rs
git commit -m "test: cover /plan plugin e2e flow and index recovery"
```

---

### Task 8: Final verification and docs alignment

**Files:**
- Modify: `docs/superpowers/specs/2026-04-28-plan-command-react-segments-design.md` (only if implementation details diverged)

- [ ] **Step 1: Run format check**

Run: `just fmt-check`  
Expected: no diff / success exit.

- [ ] **Step 2: Run lint**

Run: `just lint`  
Expected: success exit.

- [ ] **Step 3: Run test suite**

Run: `just test`  
Expected: all tests pass.

- [ ] **Step 4: Align spec if needed**

```markdown
Run:
1. `rg "plan/store|plan/session|plan/command" docs/superpowers/specs/2026-04-28-plan-command-react-segments-design.md -n`
2. `rg "plan/store|plan/session|plan/command" src/chat/commands/builtins/plan -n`
Then update spec names/paths to exactly match shipped code.
```

- [ ] **Step 5: Commit final polish**

```bash
git add src/chat/commands/builtins/plan.rs src/chat/commands/builtins/plan/command.rs src/chat/commands/builtins/plan/session.rs src/chat/commands/builtins/plan/store.rs tests/chat_command_orchestrator.rs tests/chat_plan_plugin.rs docs/superpowers/specs/2026-04-28-plan-command-react-segments-design.md
git commit -m "feat: deliver /plan plugin multi-segment react workflow"
```
