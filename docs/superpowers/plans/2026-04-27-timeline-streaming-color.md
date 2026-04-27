# Timeline Streaming Color Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add moderate color emphasis to timeline streaming/thinking text so active model output is easier to spot.

**Architecture:** Keep this as a render-layer change only. Extend timeline markdown-body rendering to optionally apply a text style override, then use that override only for `EntryKind::Thinking`. Leave runtime/provider state and users-panel rendering untouched.

**Tech Stack:** Rust, ratatui (`Line`, `Span`, `Style`), existing timeline render-entry unit tests, `cargo test`, `just fmt-check`, `just lint`, `just test`.

---

## File Structure and Responsibilities

- Modify: `src/chat/view/timeline/render_entry/markdown_body.rs`
  - Add an internal style-override path for markdown text segments (not code fences).
  - Keep existing public behavior unchanged for callers not using override.
- Modify: `src/chat/view/timeline/render_entry/status.rs`
  - Route thinking entry rendering through the style-override helper, using `theme.marker_thinking` as moderate emphasis.
- Modify: `tests/unit/chat/view/timeline/render_entry/tests.rs`
  - Add assertions that thinking body spans use highlight style and that regular assistant message spans remain unchanged.

---

### Task 1: Add a Failing Thinking-Style Regression Test (TDD)

**Files:**
- Modify: `tests/unit/chat/view/timeline/render_entry/tests.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn thinking_entry_body_uses_streaming_highlight_style() {
    let theme = crate::chat::theme::build_theme();
    let thinking = TimelineEntry::thinking("streaming reasoning text");
    let lines = render_entry(&thinking, &theme, 70);

    // Find the first non-prefix span from the first rendered line.
    let text_span_style = lines
        .first()
        .and_then(|line| line.spans.iter().skip(2).find(|span| !span.content.is_empty()))
        .map(|span| span.style)
        .expect("thinking body span should exist");

    assert_eq!(text_span_style, theme.marker_thinking);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test thinking_entry_body_uses_streaming_highlight_style -- --nocapture`  
Expected: FAIL because thinking body currently renders through default markdown text style.

- [ ] **Step 3: Add non-regression assertion for assistant message style**

```rust
#[test]
fn assistant_message_body_style_remains_default_after_thinking_color_change() {
    let theme = crate::chat::theme::build_theme();
    let entry = TimelineEntry::response("normal assistant text");
    let lines = render_entry(&entry, &theme, 70);

    let text_span_style = lines
        .first()
        .and_then(|line| line.spans.iter().skip(2).find(|span| !span.content.is_empty()))
        .map(|span| span.style)
        .expect("assistant body span should exist");

    assert_ne!(text_span_style, theme.marker_thinking);
}
```

- [ ] **Step 4: Run both focused tests**

Run: `cargo test thinking_entry_body_uses_streaming_highlight_style -- --nocapture && cargo test assistant_message_body_style_remains_default_after_thinking_color_change -- --nocapture`  
Expected: First test FAIL, second may PASS before implementation.

- [ ] **Step 5: Commit failing-test checkpoint**

```bash
git add tests/unit/chat/view/timeline/render_entry/tests.rs
git commit -m "test(timeline): add thinking streaming color regression coverage"
```

---

### Task 2: Implement Thinking Text Color Override in Render Layer (TDD Green)

**Files:**
- Modify: `src/chat/view/timeline/render_entry/markdown_body.rs`
- Modify: `src/chat/view/timeline/render_entry/status.rs`
- Modify: `tests/unit/chat/view/timeline/render_entry/tests.rs`

- [ ] **Step 1: Add markdown-body helper with optional text style override**

```rust
// in markdown_body.rs
pub(super) fn render_markdown_block_with_text_style<'a>(
    text: &str,
    theme: &ChatTheme,
    width: u16,
    first_prefix: Vec<Span<'a>>,
    cont_prefix: Vec<Span<'a>>,
    text_style: Style,
) -> Vec<Line<'a>> {
    render_markdown_block_with_mode_and_text_style(
        text,
        theme,
        width,
        first_prefix,
        cont_prefix,
        MarkdownTextMode::NormalizeParagraphs,
        Some(text_style),
    )
}
```

- [ ] **Step 2: Thread style override through internal markdown renderer**

```rust
// extend existing internal function signature
fn render_markdown_block_with_mode_and_text_style<'a>(
    text: &str,
    theme: &ChatTheme,
    width: u16,
    first_prefix: Vec<Span<'a>>,
    cont_prefix: Vec<Span<'a>>,
    text_mode: MarkdownTextMode,
    text_style_override: Option<Style>,
) -> Vec<Line<'a>> { /* ... */ }

// when processing MdSegment::Text lines
if let Some(style) = text_style_override {
    for span in &mut md_line.spans {
        span.style = style;
    }
}
```

- [ ] **Step 3: Keep existing APIs backward-compatible**

```rust
// existing helpers call the new internal function with None
render_markdown_block_with_mode_and_text_style(..., MarkdownTextMode::NormalizeParagraphs, None)
render_markdown_block_with_mode_and_text_style(..., MarkdownTextMode::PreserveLines, None)
```

- [ ] **Step 4: Apply override only to thinking entries**

```rust
// in status.rs
pub(super) fn render_thinking_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
) -> Vec<Line<'a>> {
    super::markdown_body::render_markdown_block_with_text_style(
        &entry.body,
        theme,
        width,
        vec![Span::raw(MARGIN), Span::styled("… ", theme.marker_thinking)],
        vec![Span::raw(INDENT)],
        theme.marker_thinking,
    )
}
```

- [ ] **Step 5: Run focused render-entry tests**

Run: `cargo test render_entry_ -- --nocapture`  
Expected: PASS for both new and existing render-entry tests.

- [ ] **Step 6: Commit implementation**

```bash
git add src/chat/view/timeline/render_entry/markdown_body.rs src/chat/view/timeline/render_entry/status.rs tests/unit/chat/view/timeline/render_entry/tests.rs
git commit -m "feat(timeline): colorize streaming thinking body text"
```

---

### Task 3: Full Verification and Finalize

**Files:**
- Modify: `tests/unit/chat/view/timeline/tests.rs`

- [ ] **Step 1: Run timeline-focused tests**

Run: `cargo test timeline_ -- --nocapture`  
Expected: PASS.

- [ ] **Step 2: Run repository quality gates**

Run: `just fmt-check && just lint && just test`  
Expected: all PASS.

- [ ] **Step 3: Commit final timeline assertion updates**

```bash
git add tests/unit/chat/view/timeline/tests.rs
git commit -m "test(timeline): lock streaming color behavior"
```
