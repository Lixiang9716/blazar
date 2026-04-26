# Users Three-Zone Input View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor chat users-area rendering into abstracted top/input/model panels with adjustable heights, slash-command vertical scrolling (max 6 visible commands), and an input/model separator line.

**Architecture:** Keep product state in `ChatApp` and keep rendering in view modules. Introduce a trait-based users-panel rendering boundary (similar to timeline entry rendering), with `users.rs` as the composition root. Route scroll events to command-window scrolling only during slash command mode; otherwise preserve existing timeline scroll behavior.

**Tech Stack:** Rust, ratatui, ratatui-macros layout helpers, existing `ChatApp` state model, existing integration + unit tests (`chat_render`, `tests_impl.inc`).

---

## File Structure

### Create

- `src/chat/view/users/panels.rs` — users panel renderer trait(s), render context, and shared type contracts.
- `src/chat/view/users/top_panel.rs` — top panel renderer (normal path/branch + slash command vertical window).
- `src/chat/view/users/input_panel.rs` — input panel renderer wrapper around composer rendering.
- `src/chat/view/users/model_panel.rs` — model panel renderer + model metadata line.

### Modify

- `src/chat/view/users.rs` — become users composition root with adjustable height policy and separator rendering.
- `src/chat/view/mod.rs` — keep users-area integration point unchanged except new users composition internals.
- `src/chat/view/status.rs` — remove logic moved to new panel modules (or keep only shared text utilities).
- `src/chat/view/input.rs` — either slim wrapper or move composer rendering into `input_panel.rs`.
- `src/chat/app.rs` — add users command-scroll state + helper methods.
- `src/chat/app/actions.rs` — route `ScrollUp/ScrollDown` to users command window during slash command mode.
- `src/chat/users_state.rs` — add users layout policy type(s) or users-view parameter contract.

### Test

- `tests/chat_render.rs` — update/add rendering assertions for three-zone behavior.
- `tests/unit/chat/app/tests_impl.inc` — add state/scroll routing tests.

---

### Task 1: Add users command-scroll state in ChatApp (TDD)

**Files:**
- Modify: `src/chat/app.rs`
- Modify: `src/chat/app/actions.rs`
- Test: `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Write failing app tests for slash command scroll state**

```rust
#[test]
fn slash_command_mode_scrolls_users_command_window() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    // Populate a query that has several matches
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('h'),
        KeyModifiers::NONE,
    )));

    let before = app.users_command_scroll_offset_for_test();
    app.handle_action(InputAction::ScrollDown);
    let after = app.users_command_scroll_offset_for_test();

    assert!(after >= before, "slash mode should route scroll to users command window");
}

#[test]
fn normal_mode_scroll_keeps_timeline_scroll_behavior() {
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let mut app = ChatApp::new_for_test(repo_path).expect("test app should initialize");

    let before = app.scroll_offset();
    app.handle_action(InputAction::ScrollDown);
    let after = app.scroll_offset();

    assert!(after >= before, "normal mode should keep timeline scrolling");
    assert_eq!(app.users_command_scroll_offset_for_test(), 0);
}
```

- [ ] **Step 2: Run targeted tests to confirm failure**

Run:
```bash
cargo test --test unit -- chat::app::tests_impl::slash_command_mode_scrolls_users_command_window
```

Expected: FAIL with missing `users_command_scroll_offset_for_test` or missing slash-mode scroll routing.

- [ ] **Step 3: Implement minimal ChatApp state and scroll routing**

```rust
// src/chat/app.rs (struct fields)
users_command_scroll_offset: usize,

// src/chat/app.rs (init)
users_command_scroll_offset: 0,

// src/chat/app.rs
pub(crate) fn users_command_scroll_offset(&self) -> usize {
    self.users_command_scroll_offset
}

#[cfg(test)]
pub(crate) fn users_command_scroll_offset_for_test(&self) -> usize {
    self.users_command_scroll_offset
}

pub(crate) fn scroll_users_command_window(&mut self, delta: isize) {
    let max_offset = self.inline_command_matches.len().saturating_sub(1);
    let next = if delta.is_negative() {
        self.users_command_scroll_offset.saturating_sub(delta.unsigned_abs())
    } else {
        self.users_command_scroll_offset.saturating_add(delta as usize)
    };
    self.users_command_scroll_offset = next.min(max_offset);
}
```

```rust
// src/chat/app.rs in sync_users_status_from_composer()
if let Some(query) = normalize_slash_query_static(&self.composer.lines().join("\n")) {
    self.users_status_mode = StatusMode::CommandList;
    self.refresh_inline_command_matches(&query);
    self.users_command_scroll_offset = self.users_command_scroll_offset.min(
        self.inline_command_matches.len().saturating_sub(1),
    );
} else {
    self.users_status_mode = StatusMode::Normal;
    self.inline_command_matches.clear();
    self.users_command_scroll_offset = 0;
}
```

```rust
// src/chat/app/actions.rs
InputAction::ScrollUp => {
    if self.users_status_mode == StatusMode::CommandList {
        self.scroll_users_command_window(-1);
    } else {
        self.resolve_scroll_sentinel();
        self.scroll_offset = self.scroll_offset.saturating_sub(3);
    }
}
InputAction::ScrollDown => {
    if self.users_status_mode == StatusMode::CommandList {
        self.scroll_users_command_window(1);
    } else {
        self.resolve_scroll_sentinel();
        self.scroll_offset = self.scroll_offset.saturating_add(3);
    }
}
```

- [ ] **Step 4: Re-run targeted tests**

Run:
```bash
cargo test --test unit -- chat::app::tests_impl::slash_command_mode_scrolls_users_command_window
```

Expected: PASS.

- [ ] **Step 5: Commit Task 1**

```bash
git add src/chat/app.rs src/chat/app/actions.rs tests/unit/chat/app/tests_impl.inc
git commit -m "feat(chat): add slash-command users scroll state"
```

---

### Task 2: Introduce users panel abstractions and adjustable height policy (TDD)

**Files:**
- Create: `src/chat/view/users/panels.rs`
- Modify: `src/chat/users_state.rs`
- Modify: `src/chat/view/users.rs`
- Test: `tests/chat_render.rs`

- [ ] **Step 1: Write failing render test for three-zone users area with separator**

```rust
#[test]
fn users_area_renders_top_input_model_with_separator() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let lines = render_to_lines_for_test(&mut app, 120, 24);
    let users_rows = &lines[lines.len().saturating_sub(6)..];

    assert!(
        users_rows.iter().any(|line| line.contains("~/blazar") && line.contains("main")),
        "top panel should show path + branch"
    );
    assert!(
        users_rows.iter().any(|line| line.contains("─")),
        "users area should include separator between input and model panels"
    );
    assert!(
        users_rows.iter().any(|line| line.contains("AUTO") && line.contains("echo")),
        "bottom panel should show mode and model metadata"
    );
}
```

- [ ] **Step 2: Run targeted render test and confirm failure**

Run:
```bash
cargo test --test chat_render users_area_renders_top_input_model_with_separator -q
```

Expected: FAIL because separator and panelized layout are not implemented.

- [ ] **Step 3: Add panel abstraction types and users layout policy**

```rust
// src/chat/view/users/panels.rs
use crate::chat::{app::ChatApp, theme::ChatTheme};
use ratatui_core::{layout::Rect, terminal::Frame};

#[derive(Debug, Clone, Copy)]
pub(super) struct UsersLayoutPolicy {
    pub top_min_height: u16,
    pub top_max_height: u16,
    pub input_min_height: u16,
    pub model_height: u16,
    pub command_window_max: usize,
}

impl Default for UsersLayoutPolicy {
    fn default() -> Self {
        Self {
            top_min_height: 1,
            top_max_height: 8,
            input_min_height: 1,
            model_height: 1,
            command_window_max: 6,
        }
    }
}

pub(super) struct UsersRenderContext<'a> {
    pub app: &'a ChatApp,
    pub theme: &'a ChatTheme,
    pub policy: UsersLayoutPolicy,
}

pub(super) trait UsersPanelRenderer {
    fn render(&self, frame: &mut Frame, area: Rect, ctx: &UsersRenderContext<'_>);
}
```

- [ ] **Step 4: Wire policy + abstractions into users composition root**

```rust
// src/chat/view/users.rs
mod input_panel;
mod model_panel;
mod panels;
mod top_panel;

use panels::{UsersLayoutPolicy, UsersPanelRenderer, UsersRenderContext};

let policy = UsersLayoutPolicy::default();
let ctx = UsersRenderContext { app, theme, policy };
```

- [ ] **Step 5: Re-run targeted render test**

Run:
```bash
cargo test --test chat_render users_area_renders_top_input_model_with_separator -q
```

Expected: still FAIL on behavior until panel implementations are complete (acceptable at end of Task 2).

- [ ] **Step 6: Commit Task 2**

```bash
git add src/chat/view/users.rs src/chat/view/users/panels.rs src/chat/users_state.rs tests/chat_render.rs
git commit -m "refactor(view): add users panel abstractions and layout policy"
```

---

### Task 3: Implement TopPanel with slash vertical scroll window (TDD)

**Files:**
- Create: `src/chat/view/users/top_panel.rs`
- Modify: `src/chat/view/users.rs`
- Modify: `tests/chat_render.rs`

- [ ] **Step 1: Add failing test for vertical slash command window with max 6 items**

```rust
#[test]
fn slash_mode_renders_vertical_command_window_capped_to_six_rows() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE)));

    let lines = render_to_lines_for_test(&mut app, 120, 26);
    let users_rows = &lines[lines.len().saturating_sub(10)..];

    let command_lines = users_rows
        .iter()
        .filter(|line| line.contains("/"))
        .count();
    assert!(command_lines <= 6, "top command window must cap visible items at 6");
}
```

- [ ] **Step 2: Run test to confirm failure**

Run:
```bash
cargo test --test chat_render slash_mode_renders_vertical_command_window_capped_to_six_rows -q
```

Expected: FAIL (current status row is horizontal).

- [ ] **Step 3: Implement `TopPanelRenderer`**

```rust
// src/chat/view/users/top_panel.rs
use super::panels::{UsersPanelRenderer, UsersRenderContext};
use crate::chat::users_state::StatusMode;
use ratatui_core::{layout::Rect, terminal::Frame, text::{Line, Span}};
use ratatui_widgets::paragraph::Paragraph;

pub(super) struct TopPanelRenderer;

impl UsersPanelRenderer for TopPanelRenderer {
    fn render(&self, frame: &mut Frame, area: Rect, ctx: &UsersRenderContext<'_>) {
        let snapshot = ctx.app.users_status_snapshot();
        if snapshot.status_mode != StatusMode::CommandList {
            let text = format!("{} ({})", snapshot.current_path, snapshot.branch);
            frame.render_widget(Paragraph::new(Line::from(Span::styled(text, ctx.theme.status_bar))), area);
            return;
        }

        let query = ctx.app.normalized_slash_query();
        let matches = ctx.app.inline_command_matches();
        let offset = ctx.app.users_command_scroll_offset();
        let max_rows = ctx.policy.command_window_max.min(area.height as usize);
        let end = (offset + max_rows).min(matches.len());
        let window = &matches[offset.min(matches.len())..end];

        let mut lines = vec![Line::from(Span::styled(format!("{query}"), ctx.theme.status_bar))];
        if window.is_empty() {
            lines.push(Line::from(Span::styled("No command matches", ctx.theme.dim_text)));
        } else {
            lines.extend(window.iter().map(|name| {
                Line::from(vec![
                    Span::styled("  ", ctx.theme.dim_text),
                    Span::styled(name.clone(), ctx.theme.status_right),
                ])
            }));
        }

        frame.render_widget(Paragraph::new(lines), area);
    }
}
```

- [ ] **Step 4: Re-run slash top-panel tests**

Run:
```bash
cargo test --test chat_render slash_mode_renders_inline_command_matches_in_status_row slash_mode_renders_vertical_command_window_capped_to_six_rows -q
```

Expected: PASS with updated expected behavior (rename/update legacy test as needed).

- [ ] **Step 5: Commit Task 3**

```bash
git add src/chat/view/users/top_panel.rs src/chat/view/users.rs tests/chat_render.rs
git commit -m "feat(view): render slash commands in vertical top panel window"
```

---

### Task 4: Implement InputPanel + separator + ModelPanel and users layout policy wiring (TDD)

**Files:**
- Create: `src/chat/view/users/input_panel.rs`
- Create: `src/chat/view/users/model_panel.rs`
- Modify: `src/chat/view/users.rs`
- Modify: `src/chat/view/input.rs`
- Modify: `src/chat/view/status.rs`
- Test: `tests/chat_render.rs`

- [ ] **Step 1: Add failing tests for separator and model panel persistence**

```rust
#[test]
fn users_separator_renders_between_input_and_model_panel() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let lines = render_to_lines_for_test(&mut app, 120, 24);
    assert!(lines.iter().any(|line| line.contains("─")));
}

#[test]
fn tight_height_keeps_input_and_model_panels_visible() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let lines = render_to_lines_for_test(&mut app, 100, 5);
    assert!(lines.iter().any(|line| line.contains("> ")));
    assert!(lines.iter().any(|line| line.contains("AUTO")));
}
```

- [ ] **Step 2: Run targeted tests to confirm failure**

Run:
```bash
cargo test --test chat_render users_separator_renders_between_input_and_model_panel tight_height_keeps_input_and_model_panels_visible -q
```

Expected: FAIL until separator and policy slicing are implemented.

- [ ] **Step 3: Implement InputPanel and ModelPanel renderers**

```rust
// src/chat/view/users/input_panel.rs
pub(super) struct InputPanelRenderer;
impl UsersPanelRenderer for InputPanelRenderer {
    fn render(&self, frame: &mut Frame, area: Rect, ctx: &UsersRenderContext<'_>) {
        crate::chat::view::input::render_input(frame, area, ctx.app, ctx.theme);
    }
}
```

```rust
// src/chat/view/users/model_panel.rs
pub(super) struct ModelPanelRenderer;
impl UsersPanelRenderer for ModelPanelRenderer {
    fn render(&self, frame: &mut Frame, area: Rect, ctx: &UsersRenderContext<'_>) {
        crate::chat::view::status::render_mode_config_row(frame, area, ctx.app, ctx.theme);
    }
}
```

- [ ] **Step 4: Implement policy-based layout and separator line in users.rs**

```rust
let policy = UsersLayoutPolicy::default();
let model_h = policy.model_height.min(area.height);
let separator_h = u16::from(area.height > model_h + 1);
let remaining = area.height.saturating_sub(model_h + separator_h);
let top_h = if ctx.app.users_status_snapshot().status_mode == StatusMode::CommandList {
    remaining.min(policy.top_max_height).max(policy.top_min_height)
} else {
    policy.top_min_height.min(remaining)
};
let input_h = remaining.saturating_sub(top_h).max(policy.input_min_height.min(remaining));

let [top_area, tail] = vertical![==(top_h), >=0].areas(area);
let [input_area, tail2] = vertical![==(input_h), >=0].areas(tail);
let [sep_area, model_area] = vertical![==(separator_h), ==(model_h)].areas(tail2);

top_renderer.render(frame, top_area, &ctx);
input_renderer.render(frame, input_area, &ctx);
if separator_h > 0 {
    frame.render_widget(Paragraph::new("─".repeat(sep_area.width as usize)).style(theme.dim_text), sep_area);
}
model_renderer.render(frame, model_area, &ctx);
```

- [ ] **Step 5: Re-run targeted tests**

Run:
```bash
cargo test --test chat_render users_separator_renders_between_input_and_model_panel tight_height_keeps_input_and_model_panels_visible -q
```

Expected: PASS.

- [ ] **Step 6: Commit Task 4**

```bash
git add src/chat/view/users.rs src/chat/view/users/input_panel.rs src/chat/view/users/model_panel.rs src/chat/view/input.rs src/chat/view/status.rs tests/chat_render.rs
git commit -m "refactor(view): split users area into top input model panels"
```

---

### Task 5: Add scroll-window behavior tests and slash-mode regression coverage (TDD)

**Files:**
- Modify: `tests/chat_render.rs`
- Modify: `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Add failing tests for slash command window scrolling**

```rust
#[test]
fn slash_command_window_scroll_changes_visible_items() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    app.set_composer_text("/"); // broad command match set
    let before = render_to_lines_for_test(&mut app, 120, 26);

    app.handle_action(InputAction::ScrollDown);
    let after = render_to_lines_for_test(&mut app, 120, 26);

    assert_ne!(before, after, "scrolling command window should change visible top-panel rows");
}
```

- [ ] **Step 2: Run targeted tests and confirm failure**

Run:
```bash
cargo test --test chat_render slash_command_window_scroll_changes_visible_items -q
```

Expected: FAIL until windowed rendering + offset routing are both wired.

- [ ] **Step 3: Update existing slash status assertions to the new vertical-panel contract**

```rust
// Replace horizontal status-row assertions with:
assert!(
    users_rows.iter().any(|line| line.contains("/help")),
    "top panel command window should show slash matches vertically"
);
assert!(
    users_rows.iter().all(|line| !line.contains(" · /help")),
    "new top panel should not use the old single-line horizontal slash status format"
);
```

- [ ] **Step 4: Re-run focused test group**

Run:
```bash
cargo test --quiet slash_ -- --nocapture
cargo test --quiet --test chat_render slash_ -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit Task 5**

```bash
git add tests/chat_render.rs tests/unit/chat/app/tests_impl.inc
git commit -m "test(view): cover slash command top-panel scrolling"
```

---

### Task 6: Full verification and integration commit

**Files:**
- Modify: (all files touched by Tasks 1-5)
- Test: repository quality gates

- [ ] **Step 1: Run formatting check**

Run:
```bash
just fmt-check
```

Expected: `cargo fmt --all -- --check` exits 0.

- [ ] **Step 2: Run linting**

Run:
```bash
just lint
```

Expected: clippy exits 0 with no warnings promoted to errors.

- [ ] **Step 3: Run full test suite**

Run:
```bash
just test
```

Expected: all test targets pass.

- [ ] **Step 4: Final commit**

```bash
git add src/chat/app.rs src/chat/app/actions.rs src/chat/view/users.rs src/chat/view/users/panels.rs src/chat/view/users/top_panel.rs src/chat/view/users/input_panel.rs src/chat/view/users/model_panel.rs src/chat/view/input.rs src/chat/view/status.rs src/chat/users_state.rs tests/chat_render.rs tests/unit/chat/app/tests_impl.inc
git commit -m "feat(view): refactor users area to abstract three-zone layout"
```

- [ ] **Step 5: Push branch**

```bash
git push origin master
```

Expected: remote updates cleanly without conflicts.

---

## Spec Coverage Check

1. Three-zone layout (top/input/model): covered by Tasks 2 and 4.
2. Top normal mode path/branch only: covered by Task 3.
3. Slash mode vertical command list + max 6 + scrolling: covered by Tasks 1, 3, 5.
4. Input/model separator line: covered by Task 4.
5. Adjustable height policy: covered by Task 2 + Task 4.
6. Abstraction style like render-entry: covered by Task 2 (trait boundary) and Task 4 (panel implementations).
7. Ask Tool single-panel compatibility: ensured as extension point through panel abstraction and layout policy (no immediate behavior change in this slice).

## Placeholder Scan

No TODO/TBD placeholders remain. All code-changing steps include concrete snippets, file paths, and commands.

## Type Consistency Check

The plan consistently uses:

- `users_command_scroll_offset` (ChatApp state)
- `UsersLayoutPolicy` (layout policy)
- `UsersPanelRenderer` + concrete panel renderers
- `StatusMode::CommandList` for slash-mode routing
