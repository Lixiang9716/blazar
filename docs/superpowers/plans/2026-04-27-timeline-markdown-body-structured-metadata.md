# Timeline Markdown Body + Structured Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render timeline entry正文类字段 with a shared Markdown pipeline while keeping状态/元数据字段结构化渲染 and preserving user-message plain-text behavior.

**Architecture:** Keep per-entry structured headers in existing renderers, then delegate `body` and expanded `details` to shared Markdown helpers in the timeline render-entry layer. Reuse existing fence splitting + Markdown normalization helpers to avoid duplicate parsing logic and keep behavior consistent across entry kinds.

**Tech Stack:** Rust, ratatui, ratskin/termimad pipeline already used by timeline message rendering, existing timeline unit + snapshot tests (`cargo test`, `just fmt-check`, `just lint`, `just test`)

---

## File Structure and Responsibilities

- Create: `src/chat/view/timeline/render_entry/markdown_body.rs`
  - Shared markdown body/details rendering helpers used by multiple entry renderers.
- Modify: `src/chat/view/timeline/render_entry.rs`
  - Register `markdown_body` submodule and export helper functions for internal use.
- Modify: `src/chat/view/timeline/render_entry/message.rs`
  - Keep user message plain-text behavior; route non-user body rendering to shared helper.
- Modify: `src/chat/view/timeline/render_entry/status.rs`
  - Route Warning/Hint/Thinking body rendering through shared helper.
- Modify: `src/chat/view/timeline/render_entry/tooling.rs`
  - Keep tool metadata headers structured; route ToolUse/Bash body through shared helper.
- Modify: `src/chat/view/timeline/render_entry/tooling/renderer.rs`
  - Keep ToolCall status metadata structured; route result body through shared helper.
- Modify: `src/chat/view/timeline.rs`
  - Replace details plain-text loop with shared markdown-details render path.
- Modify: `tests/unit/chat/view/timeline/render_entry/tests.rs`
  - Add focused markdown-behavior regression tests for entry kinds in scope.
- Modify: `tests/chat_render.rs`
  - Add render-level tests for expanded details markdown/diff behavior.
- Modify: `tests/chat_render_snapshot.rs`
  - Keep snapshot harness; update snapshots only if behavior changes intentionally.
- Modify: `tests/snapshots/chat_render_snapshot__*.snap` (only when expected output changes).

---

### Task 1: Add Shared Markdown Body Helper (TDD)

**Files:**
- Create: `src/chat/view/timeline/render_entry/markdown_body.rs`
- Modify: `src/chat/view/timeline/render_entry.rs`
- Test: `tests/unit/chat/view/timeline/render_entry/tests.rs`

- [ ] **Step 1: Write the failing helper test**

```rust
#[test]
fn markdown_body_helper_renders_diff_fence_lines() {
    let theme = crate::chat::theme::build_theme();
    let lines = super::markdown_body::render_markdown_block(
        "```diff\n- old\n+ new\n```",
        &theme,
        60,
        vec![Span::raw("  "), Span::raw("● ")],
        vec![Span::raw("    ")],
    );
    let text = lines_text(&lines).join("\n");
    assert!(text.contains("- old"));
    assert!(text.contains("+ new"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test markdown_body_helper_renders_diff_fence_lines`
Expected: FAIL with `could not find markdown_body` or missing function.

- [ ] **Step 3: Implement minimal shared helper module**

```rust
// src/chat/view/timeline/render_entry/markdown_body.rs
pub(super) fn render_markdown_block<'a>(
    text: &str,
    theme: &ChatTheme,
    width: u16,
    first_prefix: Vec<Span<'a>>,
    cont_prefix: Vec<Span<'a>>,
) -> Vec<Line<'a>> {
    if text.trim().is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let segments = split_code_fences(text.trim());
    let rat_skin = ratskin::RatSkin {
        skin: theme.mad_skin.clone(),
    };
    let text_width = width.saturating_sub(INDENT_WIDTH);
    let mut is_first = true;

    for segment in &segments {
        match segment {
            MdSegment::Text(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let normalized = normalize_markdown_paragraphs(trimmed);
                let parsed = rat_skin.parse(ratskin::RatSkin::parse_text(&normalized), text_width);
                for md_line in parsed {
                    let mut spans = if is_first {
                        is_first = false;
                        first_prefix.clone()
                    } else {
                        cont_prefix.clone()
                    };
                    spans.extend(md_line.spans);
                    lines.push(Line::from(spans));
                }
            }
            MdSegment::Code { lang, body } => {
                let code_lines = render_fenced_code(lang, body, theme, text_width);
                for code_line in code_lines {
                    let mut spans = if is_first {
                        is_first = false;
                        first_prefix.clone()
                    } else {
                        cont_prefix.clone()
                    };
                    spans.extend(code_line.spans);
                    lines.push(Line::from(spans));
                }
            }
        }
    }

    lines
}

pub(super) fn render_markdown_details_block<'a>(
    text: &str,
    theme: &ChatTheme,
    width: u16,
    prefix: Vec<Span<'a>>,
) -> Vec<Line<'a>> {
    render_markdown_block(text, theme, width, prefix.clone(), prefix)
}
```

- [ ] **Step 4: Wire the new module in render_entry root**

```rust
// src/chat/view/timeline/render_entry.rs
mod markdown_body;
pub(super) use markdown_body::render_markdown_details_block;
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test markdown_body_helper_renders_diff_fence_lines`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/chat/view/timeline/render_entry/markdown_body.rs src/chat/view/timeline/render_entry.rs tests/unit/chat/view/timeline/render_entry/tests.rs
git commit -m "feat(timeline): add shared markdown body render helper"
```

---

### Task 2: Migrate Message/Status Body Paths to Shared Markdown (TDD)

**Files:**
- Modify: `src/chat/view/timeline/render_entry/message.rs`
- Modify: `src/chat/view/timeline/render_entry/status.rs`
- Test: `tests/unit/chat/view/timeline/render_entry/tests.rs`

- [ ] **Step 1: Add failing behavior tests**

```rust
#[test]
fn assistant_message_uses_shared_markdown_body_renderer() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::response("## heading\n```diff\n- a\n+ b\n```");
    let text = lines_text(&render_entry(&entry, &theme, 70)).join("\n");
    assert!(text.contains("heading"));
    assert!(text.contains("- a"));
    assert!(text.contains("+ b"));
}

#[test]
fn user_message_remains_plain_text() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::user_message("**literal** not markdown");
    let text = lines_text(&render_entry(&entry, &theme, 70)).join("\n");
    assert!(text.contains("**literal**"), "user message should remain plain text");
}

#[test]
fn code_block_entry_uses_shared_markdown_body_renderer() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::code_block("diff", "- old\n+ new");
    let text = lines_text(&render_entry(&entry, &theme, 70)).join("\n");
    assert!(text.contains("- old"));
    assert!(text.contains("+ new"));
}
```

- [ ] **Step 2: Run tests to verify fail**

Run: `cargo test assistant_message_uses_shared_markdown_body_renderer && cargo test user_message_remains_plain_text && cargo test code_block_entry_uses_shared_markdown_body_renderer`
Expected: at least one FAIL before migration.

- [ ] **Step 3: Route assistant/system message body through helper**

```rust
// message.rs inside assistant branch
lines.extend(markdown_body::render_markdown_block(
    body,
    theme,
    width,
    vec![Span::raw(MARGIN), Span::styled("● ", marker_style)],
    vec![Span::raw(INDENT)],
));
```

- [ ] **Step 4: Route Warning/Hint/Thinking/CodeBlock body through helper**

```rust
// status.rs
lines.extend(markdown_body::render_markdown_block(
    &entry.body,
    theme,
    width,
    vec![Span::raw(MARGIN), Span::styled("! ", marker_style)],
    vec![Span::raw(INDENT)],
));

// status.rs in render_code_block_entry
let text = format!("```{}\n{}\n```", language, entry.body);
lines.extend(markdown_body::render_markdown_block(
    &text,
    theme,
    width,
    vec![Span::raw(INDENT)],
    vec![Span::raw(INDENT)],
));
```

- [ ] **Step 5: Run focused tests**

Run: `cargo test assistant_message_uses_shared_markdown_body_renderer && cargo test user_message_remains_plain_text`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/chat/view/timeline/render_entry/message.rs src/chat/view/timeline/render_entry/status.rs tests/unit/chat/view/timeline/render_entry/tests.rs
git commit -m "refactor(timeline): use shared markdown body for message/status entries"
```

---

### Task 3: Migrate Tool Body Paths While Keeping Metadata Structured (TDD)

**Files:**
- Modify: `src/chat/view/timeline/render_entry/tooling.rs`
- Modify: `src/chat/view/timeline/render_entry/tooling/renderer.rs`
- Test: `tests/unit/chat/view/timeline/render_entry/tests.rs`

- [ ] **Step 1: Add failing tool-body markdown tests**

```rust
#[test]
fn tool_use_body_renders_markdown_but_header_stays_structured() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::tool_use("Edit", "src/main.rs", 2, 1, "```diff\n- old\n+ new\n```");
    let lines = render_entry(&entry, &theme, 70);
    let text = lines_text(&lines).join("\n");
    assert!(text.contains("Edit"));
    assert!(text.contains("src/main.rs"));
    assert!(text.contains("+2"));
    assert!(text.contains("-1"));
    assert!(text.contains("- old"));
    assert!(text.contains("+ new"));
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test tool_use_body_renders_markdown_but_header_stays_structured`
Expected: FAIL before migration.

- [ ] **Step 3: Keep ToolUse/Bash header lines; switch body lines to helper**

```rust
// tooling.rs after structured header push
lines.extend(markdown_body::render_markdown_block(
    &entry.body,
    theme,
    width,
    vec![Span::raw(INDENT)],
    vec![Span::raw(INDENT)],
));
```

- [ ] **Step 4: Keep ToolCall status/title/subtitle structured; render result body with helper**

```rust
// tooling/renderer.rs after header/subtitle
lines.extend(markdown_body::render_markdown_block(
    &entry.body,
    theme,
    width,
    vec![Span::raw(INDENT)],
    vec![Span::raw(INDENT)],
));
```

- [ ] **Step 5: Run focused tool tests**

Run: `cargo test tool_use_body_renders_markdown_but_header_stays_structured && cargo test render_entry_renders_tool_use_and_tool_call_statuses`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/chat/view/timeline/render_entry/tooling.rs src/chat/view/timeline/render_entry/tooling/renderer.rs tests/unit/chat/view/timeline/render_entry/tests.rs
git commit -m "refactor(timeline): render tool entry bodies via shared markdown path"
```

---

### Task 4: Render Expanded Details with Markdown + Diff Support (TDD)

**Files:**
- Modify: `src/chat/view/timeline.rs`
- Modify: `tests/chat_render.rs`

- [ ] **Step 1: Add failing expanded-details markdown test**

```rust
#[test]
fn expanded_details_render_markdown_and_diff_blocks() {
    use blazar::agent::protocol::AgentEvent;
    use blazar::agent::tools::ToolKind;
    use blazar::chat::input::InputAction;

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.apply_agent_event_for_test(AgentEvent::ToolCallStarted {
        call_id: "call-1".into(),
        tool_name: "read_file".into(),
        kind: ToolKind::Local,
        arguments: "{\"path\":\"Cargo.toml\"}".into(),
        batch_id: 0,
        replay_index: 0,
        normalized_claims: Vec::new(),
    });
    app.apply_agent_event_for_test(AgentEvent::ToolCallCompleted {
        call_id: "call-1".into(),
        output: "```diff\n- old\n+ new\n```".into(),
        is_error: false,
    });
    app.handle_action(InputAction::ToggleDetails);
    let lines = render_to_lines_for_test(&mut app, 100, 24);
    let text = lines.join("\n");
    assert!(text.contains("- old"));
    assert!(text.contains("+ new"));
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test expanded_details_render_markdown_and_diff_blocks`
Expected: FAIL while details are still line-by-line plain text.

- [ ] **Step 3: Replace details plain-text loop with markdown helper call**

```rust
// timeline.rs in show_details branch
let detail_lines = render_entry::render_markdown_details_block(
    &entry.details,
    theme,
    content_width,
    vec![Span::raw(INDENT)],
);
lines.extend(detail_lines);
```

- [ ] **Step 4: Run focused details test**

Run: `cargo test expanded_details_render_markdown_and_diff_blocks`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/view/timeline.rs tests/chat_render.rs
git commit -m "feat(timeline): render expanded details through markdown pipeline"
```

---

### Task 5: Regression, Snapshot, and Full Verification

**Files:**
- Modify: `tests/chat_render_snapshot.rs` (only if harness updates are needed)
- Modify: `tests/snapshots/chat_render_snapshot__*.snap` (only if intentional output changes)

- [ ] **Step 1: Run focused render suite**

Run: `cargo test --test chat_render --test chat_render_snapshot`
Expected: PASS, or approved snapshot update required.

- [ ] **Step 2: If snapshot changed intentionally, update and verify**

Run: `INSTA_UPDATE=always cargo test --test chat_render_snapshot`
Expected: snapshot file updated and test PASS.

- [ ] **Step 3: Run full repository gates**

Run: `just fmt-check && just lint && just test`
Expected: all PASS.

- [ ] **Step 4: Commit final regression/snapshot adjustments (if any)**

```bash
git add tests/chat_render_snapshot.rs tests/snapshots/chat_render_snapshot__*.snap
git commit -m "test(timeline): refresh markdown body/details render regressions"
```
