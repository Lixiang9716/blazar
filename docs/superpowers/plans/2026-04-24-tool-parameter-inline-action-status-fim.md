# Tool Parameter Inline + Action Status + FIM Repair Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make tool-call rows render parameters inline with the tool name, remove standalone streaming UI, switch to a single action-first status surface, and add a bounded FIM-style fallback for malformed tool arguments.

**Architecture:** Keep existing Blazar-owned state and timeline entry model intact. Refactor rendering at the tool-row and top-level layout seams, then add explicit action-label state transitions in `ChatApp`. Extend runtime JSON repair with a strictly gated, single-attempt FIM correction path that runs only after deterministic repair fails and still requires strict parse/validation.

**Tech Stack:** Rust, ratatui rendering pipeline, existing `ChatApp` turn/event state machine, runtime scheduler/json_repair pipeline, cargo test/nextest, clippy, rustfmt.

---

## File Structure and Responsibilities

- Modify: `src/chat/view/timeline/render_entry/tooling/descriptor.rs`
  - Add inline parameter summary field (right-side display payload) and keep existing preview/result mode behavior.
- Modify: `src/chat/view/timeline/render_entry/tooling/renderer.rs`
  - Render one header line with left title + right parameter summary, width-safe truncation.
- Modify: `src/chat/view/timeline/render_entry/common.rs`
  - Reuse/extend argument summary extraction and unicode-safe truncation for inline parameter string.
- Modify: `src/chat/view/mod.rs`
  - Remove dedicated streaming row allocation and only render timeline + users panes.
- Delete/retire: `src/chat/view/streaming.rs`
  - Remove lower-left streaming indicator surface.
- Modify: `src/chat/app.rs`
  - Replace streaming fallback labels with action-first labels (`thinking/planning/executing/ready/error`).
- Modify: `src/chat/app/events.rs`
  - Set/clear active action labels during runtime event transitions.
- Modify: `src/chat/view/status.rs`
  - Render updated single-source action label.
- Modify: `src/agent/runtime/scheduler.rs`
  - Add fallback stage that requests FIM correction only after deterministic repair fails.
- Modify: `src/agent/runtime/json_repair.rs`
  - Add helper(s) for correction context construction and strict parse result wrapping.
- Modify: `src/agent/runtime/events.rs`
  - Add structured observability events for FIM correction request/success/failure.
- Test: `tests/unit/chat/view/timeline/render_entry/tests.rs`
  - Add assertions for inline right-side parameter rendering.
- Test: `tests/unit/chat/view/timeline/tests.rs`
  - Add assertions that streaming indicator row is absent.
- Test: `tests/unit/chat/app/tests.rs`
  - Add status-label transition assertions for action-first labels.
- Test: `tests/unit/agent/runtime/tests_impl.inc`
  - Add deterministic-first + bounded FIM fallback scenarios.

---

### Task 1: Tool Row Header Inline Parameter Rendering

**Files:**
- Modify: `tests/unit/chat/view/timeline/render_entry/tests.rs`
- Modify: `src/chat/view/timeline/render_entry/tooling/descriptor.rs`
- Modify: `src/chat/view/timeline/render_entry/tooling/renderer.rs`
- Modify: `src/chat/view/timeline/render_entry/common.rs`

- [ ] **Step 1: Write the failing render test for right-side inline parameters**

```rust
#[test]
fn tool_call_renders_parameter_inline_on_header_row() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::tool_call(
        "c-inline",
        "bash",
        ToolKind::Local,
        r#"{"command":"cargo test --all"}"#,
        "done",
        r#"{"command":"cargo test --all"}"#,
        ToolCallStatus::Success,
    );

    let lines = render_entry(&entry, &theme, 80);
    let first_line: String = lines[0]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    let second_line: String = lines.get(1).map(|l| {
        l.spans.iter().map(|s| s.content.as_ref()).collect::<String>()
    }).unwrap_or_default();

    assert!(first_line.contains("bash"));
    assert!(first_line.contains("cargo test --all"));
    assert!(
        !second_line.contains("cargo test --all"),
        "parameter summary should stay on header row instead of subtitle row"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test unit tool_call_renders_parameter_inline_on_header_row -- --exact`
Expected: FAIL because parameter text is rendered as subtitle on a separate line.

- [ ] **Step 3: Add inline parameter summary to descriptor and renderer**

```rust
// descriptor.rs (conceptual shape)
pub(crate) struct EntryDescriptor {
    pub title: String,
    pub parameter_inline: Option<String>,
    // existing fields...
}

let parameter_inline = (!subtitle.is_empty()).then_some(subtitle);
```

```rust
// renderer.rs (conceptual shape)
let mut left = vec![status_span, title_span];
let right = descriptor.parameter_inline.clone().unwrap_or_default();
let composed = compose_left_right_line(left, right, width);
lines.push(composed);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test unit tool_call_renders_parameter_inline_on_header_row -- --exact`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/unit/chat/view/timeline/render_entry/tests.rs \
  src/chat/view/timeline/render_entry/tooling/descriptor.rs \
  src/chat/view/timeline/render_entry/tooling/renderer.rs \
  src/chat/view/timeline/render_entry/common.rs
git commit -m "feat: render tool params inline on header row"
```

---

### Task 2: Remove Standalone Streaming Rendering Surface

**Files:**
- Modify: `tests/unit/chat/view/timeline/tests.rs`
- Modify: `src/chat/view/mod.rs`
- Delete: `src/chat/view/streaming.rs`

- [ ] **Step 1: Write failing test that no standalone streaming row is rendered**

```rust
#[test]
fn timeline_does_not_render_standalone_streaming_indicator_row() {
    let mut app = crate::chat::app::ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR"))
        .expect("app should initialize");
    app.send_message("hello");
    app.apply_agent_event_for_test(crate::agent::protocol::AgentEvent::ThinkingDelta {
        text: "reasoning".into(),
    });

    let lines = crate::chat::view::render_to_lines_for_test(&mut app, 90, 24);
    let text = lines.join("\n");
    assert!(
        !text.contains("streaming…"),
        "no dedicated streaming indicator row should be rendered"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test unit timeline_does_not_render_standalone_streaming_indicator_row -- --exact`
Expected: FAIL because old streaming row is still present.

- [ ] **Step 3: Remove streaming row split and indicator renderer**

```rust
// mod.rs (conceptual shape)
let [timeline_area, users_area] = vertical![>=1, ==(users_height)].areas(frame_area);
timeline::render_timeline(frame, timeline_area, app, &theme);
status::render_users(frame, users_area, app, &theme);
// remove: streaming::render_streaming_indicator(...)
```

```rust
// remove module declaration
// mod streaming;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test unit timeline_does_not_render_standalone_streaming_indicator_row -- --exact`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/unit/chat/view/timeline/tests.rs src/chat/view/mod.rs
git rm src/chat/view/streaming.rs
git commit -m "refactor: remove standalone streaming indicator surface"
```

---

### Task 3: Action-First Status Labels in Users Status Row

**Files:**
- Modify: `tests/unit/chat/app/tests.rs`
- Modify: `src/chat/app.rs`
- Modify: `src/chat/app/events.rs`
- Modify: `src/chat/view/status.rs`

- [ ] **Step 1: Write failing tests for action-label transitions**

```rust
#[test]
fn status_label_uses_action_first_states() {
    let mut app = crate::chat::app::ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR"))
        .expect("app should initialize");

    assert_eq!(app.status_label(), "ready");

    app.send_message("plan this");
    app.apply_agent_event_for_test(crate::agent::protocol::AgentEvent::ThinkingDelta {
        text: "thinking content".into(),
    });
    assert_eq!(app.status_label(), "thinking");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test unit status_label_uses_action_first_states -- --exact`
Expected: FAIL because status still falls back to `"streaming…"`.

- [ ] **Step 3: Implement action label mapping and event transitions**

```rust
// app.rs (conceptual shape)
pub fn status_label(&self) -> String {
    if let Some(label) = &self.active_action_label {
        return label.clone();
    }
    "ready".to_owned()
}
```

```rust
// events.rs (conceptual shape)
self.active_action_label = Some(match kind {
    TurnKind::Plan => "planning".to_owned(),
    TurnKind::User => "thinking".to_owned(),
    _ => "executing".to_owned(),
});
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test unit status_label_uses_action_first_states -- --exact`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/unit/chat/app/tests.rs src/chat/app.rs src/chat/app/events.rs src/chat/view/status.rs
git commit -m "feat: use action-first status labels in users row"
```

---

### Task 4: Add Bounded FIM Fallback for Tool-Argument Repair

**Files:**
- Modify: `tests/unit/agent/runtime/tests_impl.inc`
- Modify: `src/agent/runtime/json_repair.rs`
- Modify: `src/agent/runtime/scheduler.rs`
- Modify: `src/agent/runtime/events.rs`

- [ ] **Step 1: Write failing runtime tests for deterministic-first + single-attempt FIM fallback**

```rust
#[test]
fn run_turn_uses_fim_correction_only_after_deterministic_repair_fails() {
    struct FallbackNeededProvider;
    impl LlmProvider for FallbackNeededProvider {
        fn stream_turn(
            &self,
            _model: &str,
            messages: &[ProviderMessage],
            _tools: &[ToolSpec],
            tx: Sender<ProviderEvent>,
        ) {
            let has_tool_result = messages
                .iter()
                .any(|message| matches!(message, ProviderMessage::ToolResult { .. }));
            if has_tool_result {
                let _ = tx.send(ProviderEvent::TextDelta("done".into()));
                let _ = tx.send(ProviderEvent::TurnComplete);
            } else {
                let _ = tx.send(ProviderEvent::ToolCall {
                    call_id: "call-fim-ok".into(),
                    name: "bash".into(),
                    arguments: r#"{"command":"echo "broken""}"#.into(),
                });
                let _ = tx.send(ProviderEvent::TurnComplete);
            }
        }
    }

    // Arrange runtime/provider so deterministic repair fails, FIM fallback returns valid JSON.
    // Assert:
    // 1) tool executes exactly once
    // 2) captured structured events include:
    //    - tool_args_fim_correction_requested
    //    - tool_args_fim_correction_succeeded
    // 3) no duplicate fallback attempt event for the same call_id
}

#[test]
fn run_turn_rejects_invalid_fim_correction_and_returns_parse_error() {
    struct FallbackInvalidProvider;
    impl LlmProvider for FallbackInvalidProvider {
        fn stream_turn(
            &self,
            _model: &str,
            _messages: &[ProviderMessage],
            _tools: &[ToolSpec],
            tx: Sender<ProviderEvent>,
        ) {
            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-fim-bad".into(),
                name: "bash".into(),
                arguments: r#"{"command":"echo "bad""}"#.into(),
            });
            let _ = tx.send(ProviderEvent::TurnComplete);
        }
    }

    // Arrange fallback reply that remains invalid JSON.
    // Assert:
    // 1) tool execution count remains zero
    // 2) tool result sent back to model contains "JSON PARSE ERROR"
    // 3) structured events include:
    //    - tool_args_fim_correction_requested
    //    - tool_args_fim_correction_failed
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test unit run_turn_uses_fim_correction_only_after_deterministic_repair_fails -- --exact`
Expected: FAIL because no FIM fallback stage exists.

Run: `cargo test --test unit run_turn_rejects_invalid_fim_correction_and_returns_parse_error -- --exact`
Expected: FAIL because invalid correction path is not implemented.

- [ ] **Step 3: Implement constrained fallback path and observability events**

```rust
// scheduler.rs (conceptual shape)
let parsed = parse_or_repair_json(arguments);
if let Err(primary_err) = parsed {
    if should_try_fim_once(tool_name, call_signature) {
        emit_tool_args_fim_correction_requested(...);
        if let Some(corrected) = request_fim_args_correction(...).await {
            match parse_or_repair_json(&corrected) {
                Ok(ok) => {
                    emit_tool_args_fim_correction_succeeded(...);
                    // continue with validated args
                }
                Err(_) => emit_tool_args_fim_correction_failed(...),
            }
        }
    }
    // existing actionable parse error flow
}
```

```rust
// events.rs (conceptual shape)
pub fn tool_args_fim_correction_requested(...)
pub fn tool_args_fim_correction_succeeded(...)
pub fn tool_args_fim_correction_failed(...)
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test unit run_turn_uses_fim_correction_only_after_deterministic_repair_fails -- --exact`
Expected: PASS.

Run: `cargo test --test unit run_turn_rejects_invalid_fim_correction_and_returns_parse_error -- --exact`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/unit/agent/runtime/tests_impl.inc src/agent/runtime/json_repair.rs src/agent/runtime/scheduler.rs src/agent/runtime/events.rs
git commit -m "feat: add bounded FIM fallback for tool-arg repair"
```

---

### Task 5: Full Quality Gates and Final Integration Checks

**Files:**
- Modify (if needed for test fixes): files changed in Tasks 1-4 only
- Test: repository quality gates

- [ ] **Step 1: Run targeted suites first**

Run: `cargo test --test unit tool_call_renders_parameter_inline_on_header_row -- --exact`
Expected: PASS.

Run: `cargo test --test unit timeline_does_not_render_standalone_streaming_indicator_row -- --exact`
Expected: PASS.

Run: `cargo test --test unit status_label_uses_action_first_states -- --exact`
Expected: PASS.

Run: `cargo test --test unit run_turn_uses_fim_correction_only_after_deterministic_repair_fails -- --exact`
Expected: PASS.

- [ ] **Step 2: Run repository gates**

Run: `just fmt-check`
Expected: PASS.

Run: `just lint`
Expected: PASS.

Run: `just test`
Expected: PASS.

- [ ] **Step 3: Commit final polish (if any)**

```bash
git add src/chat/view/timeline/render_entry/tooling/descriptor.rs \
  src/chat/view/timeline/render_entry/tooling/renderer.rs \
  src/chat/view/timeline/render_entry/common.rs \
  src/chat/view/mod.rs \
  src/chat/app.rs \
  src/chat/app/events.rs \
  src/chat/view/status.rs \
  src/agent/runtime/json_repair.rs \
  src/agent/runtime/scheduler.rs \
  src/agent/runtime/events.rs \
  tests/unit/chat/view/timeline/render_entry/tests.rs \
  tests/unit/chat/view/timeline/tests.rs \
  tests/unit/chat/app/tests.rs \
  tests/unit/agent/runtime/tests_impl.inc
git commit -m "chore: finalize inline tool params action status and fim repair"
```

- [ ] **Step 4: Prepare merge handoff notes**

```text
Summarize:
1) inline tool parameter rendering behavior
2) removed streaming surface behavior
3) action-first status transitions
4) FIM fallback safety constraints and events
```

---

## Self-Review

1. **Spec coverage:** Plan includes tasks for inline tool parameter, streaming surface removal, action-first status labels, FIM bounded correction, and observability/testing.
2. **Placeholder scan:** No TODO/TBD placeholders; each task includes concrete files, commands, and expected outcomes.
3. **Type consistency:** Uses existing `ChatApp`/runtime seams (`status_label`, event transitions, `parse_or_repair_json`) and keeps new names scoped to tool-arg FIM correction events.
