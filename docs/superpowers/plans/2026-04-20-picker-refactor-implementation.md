# Picker Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hand-written command picker overlay/list scrolling with `tui-overlay` + `tui-widget-list` while preserving current picker behavior.

**Architecture:** Keep product state in Blazar-owned `ModalPicker`, but delegate UI mechanics to ecosystem widgets: `OverlayState` for visibility/animation and `ListState` for selection/scrolling. Refactor only picker-related paths in `picker.rs`, `view.rs`, and `app.rs` so timeline/session/git state remains untouched.

**Tech Stack:** Rust, ratatui-core, ratatui-widgets, crossterm, `tui-overlay`, `tui-widget-list`, cargo/just quality gates.

---

## File Structure / Responsibility Map

- **Modify:** `Cargo.toml`
  - Add picker refactor dependencies.
- **Modify:** `src/chat/picker.rs`
  - Keep picker data/filter ownership in Blazar state.
  - Replace manual scroll/visible state with widget states.
- **Modify:** `src/chat/view.rs`
  - Replace manual picker rendering with `Overlay` + `ListView`.
- **Modify:** `src/chat/app.rs`
  - Route picker actions to new picker state API and drive overlay animation tick.
- **Modify:** `tests/chat_render.rs`
  - Add picker behavior regression tests (open, navigation, scroll visibility).

---

### Task 1: Add regression tests for picker behavior (TDD first)

**Files:**
- Modify: `tests/chat_render.rs`
- Test: `tests/chat_render.rs`

- [ ] **Step 1: Write failing test for opening picker with `/`**

```rust
#[test]
fn slash_opens_command_picker_overlay() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = ChatApp::new_for_test(REPO_ROOT);
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    let lines = render_to_lines_for_test(&app, 100, 35);
    assert!(lines.iter().any(|l| l.contains("Commands")));
    assert!(lines.iter().any(|l| l.contains("/help")));
}
```

- [ ] **Step 2: Write failing test for navigating down to later commands**

```rust
#[test]
fn picker_navigation_reaches_later_commands() {
    use blazar::chat::input::InputAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = ChatApp::new_for_test(REPO_ROOT);
    app.handle_action(InputAction::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));

    for _ in 0..12 {
        app.handle_action(InputAction::PickerDown);
    }

    let lines = render_to_lines_for_test(&app, 100, 35);
    assert!(lines.iter().any(|l| l.contains("/tools") || l.contains("/agents")));
}
```

- [ ] **Step 3: Run tests and confirm at least one fails before refactor**

Run:
```bash
cargo test --test chat_render slash_opens_command_picker_overlay picker_navigation_reaches_later_commands -q
```

Expected: FAIL on missing/new behavior assertions before implementation is complete.

- [ ] **Step 4: Commit failing tests**

```bash
git add tests/chat_render.rs
git commit -m "test(chat): add picker overlay regression tests"
```

---

### Task 2: Add dependencies and refactor picker state model

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/chat/picker.rs`
- Test: `tests/chat_render.rs`

- [ ] **Step 1: Add crate dependencies**

```toml
[dependencies]
tui-overlay = "0.1"
tui-widget-list = "0.15"
```

- [ ] **Step 2: Replace manual picker state with widget-managed state**

Implement in `src/chat/picker.rs`:

```rust
use tui_overlay::OverlayState;
use tui_widget_list::ListState;

#[derive(Debug, Clone)]
pub struct ModalPicker {
    pub title: String,
    pub items: Vec<PickerItem>,
    pub filter: String,
    pub list_state: ListState,
    pub overlay_state: OverlayState,
}
```

- [ ] **Step 3: Update methods to use new state fields**

```rust
pub fn open(&mut self) {
    self.filter.clear();
    self.list_state.select(Some(0));
    self.overlay_state.open();
}

pub fn close(&mut self) {
    self.filter.clear();
    self.overlay_state.close();
}

pub fn is_visible(&self) -> bool {
    self.overlay_state.is_open() || self.overlay_state.is_animating()
}
```

- [ ] **Step 4: Replace `move_up` / `move_down` / `select_current` with list-state logic**

```rust
pub fn move_up(&mut self) {
    self.list_state.previous();
}

pub fn move_down(&mut self, count: usize) {
    if count > 0 {
        self.list_state.next();
    }
}

pub fn select_current(&self) -> Option<String> {
    let filtered = self.filtered_items();
    self.list_state
        .selected()
        .and_then(|idx| filtered.get(idx))
        .map(|item| item.label.clone())
}
```

- [ ] **Step 5: Reset selection on filter updates**

```rust
pub fn push_filter(&mut self, ch: char) {
    self.filter.push(ch);
    self.list_state.select(Some(0));
}

pub fn pop_filter(&mut self) {
    self.filter.pop();
    self.list_state.select(Some(0));
}
```

- [ ] **Step 6: Run target tests**

Run:
```bash
cargo test --test chat_render slash_opens_command_picker_overlay picker_navigation_reaches_later_commands -q
```

Expected: may still FAIL until rendering is refactored in Task 3.

- [ ] **Step 7: Commit state refactor**

```bash
git add Cargo.toml src/chat/picker.rs
git commit -m "refactor(chat): move picker state to overlay/list widgets"
```

---

### Task 3: Replace manual picker rendering with Overlay + ListView

**Files:**
- Modify: `src/chat/view.rs`
- Test: `tests/chat_render.rs`

- [ ] **Step 1: Update imports for overlay/list widgets**

Add imports:

```rust
use tui_overlay::{Anchor, Backdrop, Overlay, Slide};
use tui_widget_list::{ListBuilder, ListView};
```

- [ ] **Step 2: Replace `if app.picker.visible` gate**

```rust
if app.picker.is_visible() {
    render_picker(frame, area, app, &theme);
}
```

- [ ] **Step 3: Rewrite `render_picker` to render overlay chrome first**

```rust
let overlay = Overlay::new()
    .anchor(Anchor::BottomLeft)
    .slide(Slide::Bottom)
    .width(Constraint::Length(50))
    .height(Constraint::Length((PICKER_PAGE_SIZE as u16) + 4))
    .backdrop(Backdrop::new(Color::Rgb(0, 0, 0)))
    .block(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(theme.picker_title)
            .title(format!(" {} ", app.picker.title)),
    );
```

- [ ] **Step 4: Render filtered command list with `ListView`**

```rust
let filtered = app.picker.filtered_items();
let builder = ListBuilder::new(|ctx| {
    let item = filtered[ctx.index];
    let style = if ctx.is_selected {
        theme.picker_selected
    } else {
        theme.picker_item
    };
    let marker = if ctx.is_selected { "› " } else { "  " };
    (
        Line::from(vec![
            Span::styled(marker, style),
            Span::styled(item.label.clone(), style),
            Span::styled(format!("  {}", item.description), theme.picker_desc),
        ]),
        1,
    )
});
let list = ListView::new(builder, filtered.len()).infinite_scrolling(true);
```

- [ ] **Step 5: Keep existing footer UX**

```rust
let footer = Line::from(Span::styled(
    "↑↓ navigate · enter select · esc cancel",
    theme.dim_text,
));
```

- [ ] **Step 6: Run picker render tests**

Run:
```bash
cargo test --test chat_render slash_opens_command_picker_overlay picker_navigation_reaches_later_commands -q
```

Expected: PASS.

- [ ] **Step 7: Commit render refactor**

```bash
git add src/chat/view.rs
git commit -m "refactor(chat): render picker with tui-overlay and tui-widget-list"
```

---

### Task 4: Wire picker input/tick flow in ChatApp

**Files:**
- Modify: `src/chat/app.rs`
- Test: `tests/chat_render.rs`

- [ ] **Step 1: Route picker actions through new methods/signatures**

Update in picker branch:

```rust
if self.picker.is_visible() {
    match action {
        InputAction::ScrollUp | InputAction::PickerUp => self.picker.move_up(),
        InputAction::ScrollDown | InputAction::PickerDown => {
            let count = self.picker.filtered_items().len();
            self.picker.move_down(count);
        }
        // ...
    }
}
```

- [ ] **Step 2: Open picker via `open()` and close via `close()` only**

```rust
if let KeyCode::Char('/') = key.code && self.composer_text().is_empty() {
    self.picker.open();
    return;
}
```

- [ ] **Step 3: Drive overlay animation in `tick()`**

```rust
// use a fixed delta tied to loop cadence for now
self.picker.overlay_state.tick(Duration::from_millis(100));
```

- [ ] **Step 4: Run focused app behavior tests**

Run:
```bash
cargo test --test chat_render interactive_send_message_shows_echo_response -q
cargo test --test chat_render slash_opens_command_picker_overlay picker_navigation_reaches_later_commands -q
```

Expected: PASS.

- [ ] **Step 5: Commit app wiring**

```bash
git add src/chat/app.rs
git commit -m "refactor(chat): wire picker input flow to overlay/list state"
```

---

### Task 5: Full validation and cleanup

**Files:**
- Modify: none (unless fixes are required)
- Test: repository quality gates

- [ ] **Step 1: Format check**

Run:
```bash
just fmt-check
```

Expected: exit code 0.

- [ ] **Step 2: Lint**

Run:
```bash
just lint
```

Expected: exit code 0, no clippy warnings.

- [ ] **Step 3: Test suite**

Run:
```bash
just test
```

Expected: all tests pass.

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "refactor(chat): migrate picker to tui-overlay + tui-widget-list"
```

- [ ] **Step 5: Push branch**

```bash
git push
```

Expected: remote branch updated successfully.

---

## Spec Coverage Check

- Overlay refactor requirement: covered in **Task 3**.
- List scroll refactor requirement: covered in **Task 2 + Task 3 + Task 4**.
- Preserve command/filter/select behavior: covered in **Task 1 + Task 2 + Task 4**.
- Keep state ownership in Blazar types: covered in **Task 2** (`ModalPicker` remains owner).
- Quality gates (`fmt/lint/test`): covered in **Task 5**.

## Placeholder Scan

- No `TODO`, `TBD`, or deferred steps.
- Every code-change step includes concrete snippet and concrete file path.
- Every verification step includes exact command and expected result.

## Type Consistency Check

- `ModalPicker` remains the state boundary in all tasks.
- Visibility checks use `is_visible()` consistently.
- Selection reads from `list_state.selected()` consistently.
- Action routing and method names are consistent across `picker.rs`, `view.rs`, `app.rs`.
