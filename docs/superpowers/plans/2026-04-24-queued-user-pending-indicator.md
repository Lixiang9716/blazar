# Queued User Pending Indicator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show queued user input immediately as `xxx (pending)` in timeline style during streaming, while keeping real dispatch in the next turn cycle.

**Architecture:** Keep queue truth in `ChatApp.pending_messages` and render pending rows as derived view output (not persisted `TimelineEntry`). Add a read-only accessor from app state to the renderer, append pending user-style lines in timeline render, and keep existing dispatch/materialization semantics unchanged. Cover rendering and queue lifecycle with focused tests so assistant streaming continuity stays intact.

**Tech Stack:** Rust, ratatui, existing chat runtime/timeline renderer, cargo test, just

---

## File Structure and Responsibilities

- Modify: `src/chat/app.rs`
  - Add read-only accessor exposing queued user text for rendering.
- Modify: `src/chat/view/timeline.rs`
  - Append derived pending rows (`(pending)`) after normal timeline entries.
- Modify: `tests/unit/chat/app/tests_impl.inc`
  - Validate accessor data and queue-driven pending lifecycle assumptions.
- Modify: `tests/chat_render.rs`
  - Verify pending rows appear in render and disappear after dispatch.

### Task 1: Expose queued user text for rendering

**Files:**
- Modify: `src/chat/app.rs`
- Test: `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Write failing unit test for queued preview accessor**

```rust
#[test]
fn queued_user_texts_for_render_reflects_pending_queue_fifo() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.send_message("first");
    app.apply_agent_event_for_test(AgentEvent::TurnStarted {
        turn_id: "busy-turn".into(),
    });

    app.send_message("second");
    app.send_message("third");

    assert_eq!(
        app.queued_user_texts_for_render(),
        vec!["second".to_string(), "third".to_string()]
    );
}
```

- [ ] **Step 2: Run test to confirm fail**

Run: `cargo test --lib chat::app::tests::queued_user_texts_for_render_reflects_pending_queue_fifo -- --exact`  
Expected: FAIL with missing method `queued_user_texts_for_render`.

- [ ] **Step 3: Implement minimal accessor in app state**

```rust
// src/chat/app.rs (inside impl ChatApp)
pub(crate) fn queued_user_texts_for_render(&self) -> Vec<String> {
    self.pending_messages
        .iter()
        .map(|turn| turn.user_text.clone())
        .collect()
}
```

- [ ] **Step 4: Re-run test to confirm pass**

Run: `cargo test --lib chat::app::tests::queued_user_texts_for_render_reflects_pending_queue_fifo -- --exact`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/app.rs tests/unit/chat/app/tests_impl.inc
git commit -m "feat(chat): expose queued user text for pending render"
```

### Task 2: Render `xxx (pending)` rows in timeline style

**Files:**
- Modify: `src/chat/view/timeline.rs`
- Test: `tests/chat_render.rs`

- [ ] **Step 1: Write failing render test for pending marker visibility**

```rust
#[test]
fn chat_view_renders_pending_user_rows_while_busy() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)));
    app.handle_action(InputAction::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)));
    app.handle_action(InputAction::Submit);
    app.apply_agent_event_for_test(blazar::agent::protocol::AgentEvent::TurnStarted {
        turn_id: "streaming".into(),
    });

    app.send_message("queued while busy");
    let lines = render_to_lines_for_test(&mut app, 100, 35);

    assert!(lines.iter().any(|line| line.contains("queued while busy (pending)")));
}
```

- [ ] **Step 2: Run failing render test**

Run: `cargo test chat_view_renders_pending_user_rows_while_busy -- --exact`  
Expected: FAIL because pending row is not rendered yet.

- [ ] **Step 3: Implement pending row rendering in timeline renderer**

```rust
// src/chat/view/timeline.rs (inside render_timeline, after normal entry loop)
for pending_text in app.queued_user_texts_for_render() {
    let rendered = format!("{pending_text} (pending)");
    lines.push(Line::from(vec![
        Span::raw(MARGIN),
        Span::styled("› ", theme.marker_response),
        Span::styled(rendered, theme.bold_text),
    ]));
    lines.push(Line::from(""));
}
```

- [ ] **Step 4: Re-run render test**

Run: `cargo test chat_view_renders_pending_user_rows_while_busy -- --exact`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/view/timeline.rs tests/chat_render.rs
git commit -m "feat(chat): render queued input as pending timeline rows"
```

### Task 3: Validate pending row lifecycle and non-splitting behavior

**Files:**
- Modify: `tests/chat_render.rs`
- Modify: `tests/unit/chat/app/tests_impl.inc` (if needed for queue lifecycle assertions)

- [ ] **Step 1: Add failing lifecycle test (pending disappears after dispatch)**

```rust
#[test]
fn pending_row_disappears_after_queue_dispatch() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.send_message("first");
    app.apply_agent_event_for_test(blazar::agent::protocol::AgentEvent::TurnStarted {
        turn_id: "busy".into(),
    });
    app.send_message("after");

    let before = render_to_lines_for_test(&mut app, 100, 35);
    assert!(before.iter().any(|line| line.contains("after (pending)")));

    app.apply_agent_event_for_test(blazar::agent::protocol::AgentEvent::TurnComplete);
    let after = render_to_lines_for_test(&mut app, 100, 35);
    assert!(!after.iter().any(|line| line.contains("after (pending)")));
    assert!(after.iter().any(|line| line.contains("after")));
}
```

- [ ] **Step 2: Run lifecycle and continuity tests**

Run:  
`cargo test pending_row_disappears_after_queue_dispatch -- --exact && cargo test --lib chat::app::tests::streaming_text_not_split_by_queued_user_input -- --exact`  
Expected: first FAIL then PASS after implementation; continuity test remains PASS throughout.

- [ ] **Step 3: Adjust implementation only if lifecycle test reveals gaps**

```rust
// Keep queue lifecycle source-of-truth in pending_messages;
// do not add persistent pending timeline entries.
```

- [ ] **Step 4: Re-run targeted test set**

Run:  
`cargo test --lib chat::app::tests::queued_user_texts_for_render_reflects_pending_queue_fifo -- --exact && cargo test chat_view_renders_pending_user_rows_while_busy -- --exact && cargo test pending_row_disappears_after_queue_dispatch -- --exact && cargo test --lib chat::app::tests::streaming_text_not_split_by_queued_user_input -- --exact`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/chat_render.rs tests/unit/chat/app/tests_impl.inc
git commit -m "test(chat): cover queued pending row lifecycle"
```

### Task 4: Run repository quality gates

**Files:**
- Modify: none expected (unless failures require fixes)

- [ ] **Step 1: Run format check**

Run: `just fmt-check`  
Expected: PASS.

- [ ] **Step 2: Run lint**

Run: `just lint`  
Expected: PASS.

- [ ] **Step 3: Run full tests**

Run: `just test`  
Expected: PASS.

- [ ] **Step 4: If fixes needed, apply minimal correction + test**

```rust
// Apply only failure-driven minimal corrections.
```

- [ ] **Step 5: Commit any gate-driven fix (only if needed)**

```bash
git add <changed-files>
git commit -m "fix(chat): resolve pending indicator gate regressions"
```

## Plan Self-Review

### 1. Spec coverage

- Pending marker visibility `xxx (pending)`: Task 2.
- Dispatch-time-only real input: Task 1 + Task 3.
- Streaming continuity unchanged: Task 3.
- Queue continuation semantics unchanged: preserved and validated in Task 3 by existing continuity regression.

### 2. Placeholder scan

- No unresolved placeholders.
- Each code-changing step includes concrete code snippets and exact commands.

### 3. Type consistency

- Accessor name is consistent: `queued_user_texts_for_render`.
- Pending marker text format is consistent: `(<pending>)` suffix shown as `" (pending)"`.
- Tests reference same method and marker text across tasks.
