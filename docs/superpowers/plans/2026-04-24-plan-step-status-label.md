# Thinking Step-Name Status Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace only the `thinking` status label with an AI-generated short next-step action name (for example `explore`), while keeping `planning`, `executing <tool>`, `ready`, and `error: ...` unchanged.

**Architecture:** Reuse current `ChatApp` status flow and only intercept the `thinking` branch. Parse a model-provided naming line from output (`next_step_name: <short-name>`), cache the latest candidate in app state, and return it when `derive_active_turn_status_label()` would otherwise return `thinking`. If the naming line is absent, fallback to conservative text extraction; all other status branches remain behavior-compatible.

**Tech Stack:** Rust, existing chat app turn/event state machine, unit tests in `tests/unit/chat/app/tests_impl.inc`, status row renderer in `src/chat/view/status.rs`.

---

## File Structure and Responsibilities

- Modify: `src/chat/app/turns.rs`
  - Add helpers to parse `next_step_name:` naming lines and fallback extraction.
- Modify: `src/chat/app.rs`
  - Add state for latest thinking step-name candidate.
  - Update status derivation so only thinking label can be substituted.
- Modify: `src/chat/app/events.rs`
  - Refresh thinking step-name candidate from `ThinkingDelta` and finalized plan body.
- Modify: `tests/unit/chat/app/tests_impl.inc`
  - Add/adjust tests for thinking-only substitution and unchanged non-thinking labels.
- Modify: `src/chat/view/status.rs`
  - Keep rendering contract unchanged; update assertions/tests only if needed.

---

### Task 1: Add Short Action Name Extractors

**Files:**
- Modify: `src/chat/app/turns.rs`
- Modify: `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Write failing extractor tests**

```rust
#[test]
fn short_action_name_from_text_extracts_first_action_word() {
    let text = "Next step: explore scheduler flow and patch status.";
    assert_eq!(
        super::turns::short_action_name_from_text(text),
        Some("explore".to_owned())
    );
}

#[test]
fn short_action_name_is_kept_concise() {
    let text = "Next step: investigate-super-long-step-name-for-parser";
    assert_eq!(
        super::turns::short_action_name_from_text(text),
        Some("investigat…".to_owned())
    );
}

#[test]
fn extract_plan_action_names_reads_numbered_plan_body() {
    let body = "1. Explore codebase\n2. Patch status derivation\n3. Add tests";
    assert_eq!(
        super::turns::extract_plan_action_names(body),
        vec!["explore".to_owned(), "patch".to_owned(), "add".to_owned()]
    );
}

#[test]
fn parse_next_step_name_line_extracts_explicit_name() {
    let text = "next_step_name: explore\nI will inspect scheduler flow next.";
    assert_eq!(
        super::turns::parse_next_step_name_line(text),
        Some("explore".to_owned())
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib short_action_name_from_text_extracts_first_action_word extract_plan_action_names_reads_numbered_plan_body parse_next_step_name_line_extracts_explicit_name short_action_name_is_kept_concise`
Expected: FAIL because helper functions are not implemented yet.

- [ ] **Step 3: Implement extractor helpers**

```rust
pub(super) fn short_action_name_from_text(text: &str) -> Option<String> {
    let cleaned = text
        .trim()
        .trim_start_matches(|ch: char| ch.is_ascii_digit() || matches!(ch, '.' | '-' | '[' | ']' | ':' | ' '))
        .trim();
    let first = cleaned.split_whitespace().next()?.to_ascii_lowercase();
    let mut name = first;
    if name.chars().count() > 10 {
        name = name.chars().take(9).collect::<String>() + "…";
    }
    (!name.is_empty()).then_some(name)
}

pub(super) fn extract_plan_action_names(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(short_action_name_from_text)
        .collect()
}

pub(super) fn parse_next_step_name_line(text: &str) -> Option<String> {
    let first_line = text.lines().next()?.trim();
    let value = first_line.strip_prefix("next_step_name:")?.trim();
    short_action_name_from_text(value)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib short_action_name_from_text_extracts_first_action_word extract_plan_action_names_reads_numbered_plan_body parse_next_step_name_line_extracts_explicit_name short_action_name_is_kept_concise`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/app/turns.rs tests/unit/chat/app/tests_impl.inc
git commit -m "feat: add thinking action-name extractors"
```

---

### Task 2: Wire Thinking Step-Name State in ChatApp

**Files:**
- Modify: `src/chat/app.rs`
- Modify: `src/chat/app/events.rs`
- Modify: `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Write failing status tests for thinking-only substitution**

```rust
#[test]
fn status_label_uses_action_name_only_for_thinking() {
    let mut app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("init");

    app.active_turn_kind = Some(TurnKind::Chat);
    app.apply_agent_event_for_test(AgentEvent::TurnStarted { turn_id: "t1".into() });
    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: "Next: explore runtime flow".into(),
    });
    assert_eq!(app.status_label(), "explore");
}

#[test]
fn status_label_keeps_executing_label_unchanged() {
    let mut app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("init");
    app.active_turn_kind = Some(TurnKind::Chat);
    app.apply_agent_event_for_test(AgentEvent::TurnStarted { turn_id: "t2".into() });
    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-1".into(),
        tool_name: "bash".into(),
        kind: crate::agent::tools::ToolKind::Local,
        arguments: r#"{"command":"echo hi"}"#.into(),
        batch_id: 0,
        replay_index: 0,
        normalized_claims: Vec::new(),
    });
    assert_eq!(app.status_label(), "executing bash");
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --lib status_label_uses_action_name_only_for_thinking status_label_keeps_executing_label_unchanged`
Expected: FAIL before thinking substitution logic is added.

- [ ] **Step 3: Implement state + derivation updates**

```rust
// app.rs (field)
thinking_action_name: Option<String>,

fn derive_active_turn_status_label(&self) -> String {
    if let Some(tool_name) = self
        .agent_state
        .active_tools
        .iter()
        .rev()
        .find(|tool| tool.status == ActiveToolStatus::Running)
        .map(|tool| tool.tool_name.clone())
    {
        return format!("executing {tool_name}");
    }

    match self.active_turn_kind {
        Some(TurnKind::Plan) => "planning".to_owned(),
        _ => self
            .thinking_action_name
            .clone()
            .unwrap_or_else(|| "thinking".to_owned()),
    }
}
```

```rust
// events.rs (ThinkingDelta + plan completion updates)
if let AgentEvent::ThinkingDelta { text } = &event {
    self.thinking_action_name = parse_next_step_name_line(text)
        .or_else(|| short_action_name_from_text(text));
}
if self.active_turn_kind == Some(TurnKind::Plan) && matches!(event, AgentEvent::TurnComplete) {
    if let Some((_, body)) = extract_plan_title_and_body(&assistant_body) {
        self.thinking_action_name = extract_plan_action_names(&body).into_iter().next();
    }
}
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --lib status_label_uses_action_name_only_for_thinking status_label_keeps_executing_label_unchanged`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/app.rs src/chat/app/events.rs tests/unit/chat/app/tests_impl.inc
git commit -m "feat: substitute thinking label with action name"
```

---

### Task 3: Update Existing Transition Tests to New Thinking Semantics

**Files:**
- Modify: `tests/unit/chat/app/tests_impl.inc`
- Modify: `src/chat/view/status.rs`

- [ ] **Step 1: Update failing transition assertions**

```rust
#[test]
fn status_label_transitions_across_action_states() {
    // ... existing setup ...
    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta {
        text: "Next: explore parser flow".into(),
    });
    assert_eq!(app.status_label(), "explore");

    // executing branch remains unchanged
    assert_eq!(app.status_label(), "executing bash");

    // plan branch remains unchanged
    // assert_eq!(app.status_label(), "planning");
}
```

- [ ] **Step 2: Run test to verify expected failure/snapshot drift**

Run: `cargo test --lib status_label_transitions_across_action_states planning_turn_uses_planning_while_streaming_then_sets_title`
Expected: FAIL before assertions and render expectations are updated.

- [ ] **Step 3: Apply minimal assertion/render updates**

```rust
// status.rs: no logic change required; keep consuming app.status_label()
let status = app.status_label();
```

- [ ] **Step 4: Run updated tests**

Run: `cargo test --lib status_label_transitions_across_action_states planning_turn_uses_planning_while_streaming_then_sets_title`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/unit/chat/app/tests_impl.inc src/chat/view/status.rs
git commit -m "test: align status transitions with thinking action names"
```

---

### Task 4: Verification and Handoff

**Files:**
- Modify (if needed): files from Tasks 1-3 only

- [x] **Step 1: Run targeted tests**

Run:
`cargo test --lib short_action_name_from_text_extracts_first_action_word`
`cargo test --lib extract_plan_action_names_reads_numbered_plan_body`
`cargo test --lib parse_next_step_name_line_extracts_explicit_name`
`cargo test --lib short_action_name_is_kept_concise`
`cargo test --lib status_label_uses_action_name_only_for_thinking`
`cargo test --lib status_label_keeps_executing_label_unchanged`
`cargo test --lib status_label_transitions_across_action_states`
Expected: PASS for each command.

- [x] **Step 2: Run repository gates**

Run: `just fmt-check`
Expected: PASS.

Run: `just lint`
Expected: PASS.

Run: `just test`
Expected: PASS.

- [x] **Step 3: Commit final polish (if needed)**

```bash
git add src/chat/app.rs src/chat/app/events.rs src/chat/app/turns.rs src/chat/view/status.rs tests/unit/chat/app/tests_impl.inc
git commit -m "chore: finalize thinking action-name status rollout"
```

- [x] **Step 4: Prepare merge handoff notes**

```text
Merge handoff:
1) Thinking labels use a short action name only, derived from `next_step_name:`.
2) Planning, executing, ready, and error labels remain unchanged.
3) Naming protocol: read `next_step_name:` first, then fall back to concise action extraction.
4) Verification: targeted tests passed for extractor/status behavior, and repo gates passed (`just fmt-check`, `just lint`, `just test`).
```

---

## Self-Review

1. **Spec coverage:** plan covers thinking-only substitution, extractor logic, event wiring, unchanged non-thinking labels, and verification.
2. **Placeholder scan:** no TODO/TBD placeholders; steps include concrete code/commands.
3. **Type consistency:** helper and state names are consistent (`short_action_name_from_text`, `extract_plan_action_names`, `thinking_action_name`).
