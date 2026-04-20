# TUI Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split bloated app.rs/view.rs into focused modules, integrate opaline theme engine for 39-theme runtime switching.

**Architecture:** Extract demo playback and event loop from app.rs into standalone modules. Convert view.rs into a view/ directory with one file per UI section. Replace hand-coded Solarized colors with opaline semantic token resolution while keeping ChatTheme as the owned rendering contract.

**Tech Stack:** Rust, ratatui-core 0.1, opaline 0.4 (ratatui feature), tui-overlay, tui-widget-list

---

### Task 1: Extract demo.rs from app.rs

**Files:**
- Create: `src/chat/demo.rs`
- Modify: `src/chat/app.rs`
- Modify: `src/chat/mod.rs`

- [ ] **Step 1: Create `src/chat/demo.rs` with demo functions**

Move `demo_timeline()` and `demo_playback_script()` from `src/chat/app.rs` to `src/chat/demo.rs`. The new file needs its own imports:

```rust
//! Demo timeline entries for visual testing and playback.

use crate::chat::model::TimelineEntry;

/// Demo timeline entries for visual testing (BLAZAR_DEMO env var).
pub fn demo_timeline() -> Vec<TimelineEntry> {
    demo_playback_script().into_iter().take(3).collect()
}

/// Full demo playback script — one entry per second when triggered by "1".
pub fn demo_playback_script() -> Vec<TimelineEntry> {
    // ... (move the entire vec![...] body here, unchanged)
}
```

- [ ] **Step 2: Update `src/chat/app.rs` — remove demo functions, add import**

Remove `fn demo_timeline()` and `fn demo_playback_script()` (lines 307-511). Replace all internal usages with `crate::chat::demo::demo_timeline()` and `crate::chat::demo::demo_playback_script()`. Affected call sites:
- `ChatApp::new()` line 37: `demo_timeline()` → `crate::chat::demo::demo_timeline()`
- `ChatApp::new_with_demo_timeline()` line 80: same
- `ChatApp::send_message()` line 163: `demo_playback_script()` → `crate::chat::demo::demo_playback_script()`

- [ ] **Step 3: Register module in `src/chat/mod.rs`**

Add `pub mod demo;` to `src/chat/mod.rs`.

- [ ] **Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: All tests pass, no behavior change.

- [ ] **Step 5: Commit**

```bash
git add src/chat/demo.rs src/chat/app.rs src/chat/mod.rs
git commit -m "refactor(chat): extract demo playback into dedicated module"
```

---

### Task 2: Extract event_loop.rs from app.rs

**Files:**
- Create: `src/chat/event_loop.rs`
- Modify: `src/chat/app.rs`
- Modify: `src/chat/mod.rs`
- Modify: `src/app.rs` (caller)

- [ ] **Step 1: Create `src/chat/event_loop.rs`**

Move `run_terminal_chat()`, `TerminalGuard`, and `resolve_repo_path()` from `src/chat/app.rs`:

```rust
//! Terminal event loop and lifecycle management.

use crate::chat::app::ChatApp;
use crate::chat::input::InputAction;
use crate::chat::view::render_frame;
use crate::config::MascotConfig;
use crossterm::{
    ExecutableCommand,
    event::{self, EnableMouseCapture, Event, MouseEventKind},
    terminal::{EnterAlternateScreen, enable_raw_mode},
};
use ratatui_core::terminal::Terminal;
use ratatui_crossterm::CrosstermBackend;
use serde_json::Value;
use std::io::stdout;
use std::time::{Duration, Instant};

pub fn run_terminal_chat(
    schema: Value,
    _mascot: MascotConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    // ... (move body unchanged)
}

/// Restores raw mode and alternate screen when dropped.
pub(crate) struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // ... (move body unchanged)
    }
}

/// Extracts the repository path from the schema JSON.
pub fn resolve_repo_path(schema: &Value) -> String {
    // ... (move body unchanged)
}
```

- [ ] **Step 2: Update `src/chat/app.rs` — remove moved functions**

Remove `run_terminal_chat()`, `TerminalGuard`, and `resolve_repo_path()` from app.rs. The `use` statements specific to them (crossterm, Terminal, etc.) can also be removed from app.rs.

- [ ] **Step 3: Register module and update caller**

Add `pub mod event_loop;` to `src/chat/mod.rs`.

Update `src/app.rs` line 338:
```rust
// Before:
chat::app::run_terminal_chat(schema, mascot)
// After:
chat::event_loop::run_terminal_chat(schema, mascot)
```

- [ ] **Step 4: Update test imports**

`tests/chat_runtime.rs` imports `blazar::chat::app::resolve_repo_path`. Update to:
```rust
use blazar::chat::event_loop::resolve_repo_path;
```

- [ ] **Step 5: Build and test**

Run: `cargo build && cargo test`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/chat/event_loop.rs src/chat/app.rs src/chat/mod.rs src/app.rs tests/chat_runtime.rs
git commit -m "refactor(chat): extract event loop into dedicated module"
```

---

### Task 3: Split view.rs into view/ module directory

**Files:**
- Create: `src/chat/view/mod.rs`
- Create: `src/chat/view/banner.rs`
- Create: `src/chat/view/timeline.rs`
- Create: `src/chat/view/input.rs`
- Create: `src/chat/view/status.rs`
- Create: `src/chat/view/picker.rs`
- Delete: `src/chat/view.rs` (replaced by directory)

- [ ] **Step 1: Create view/ directory and mod.rs**

Create `src/chat/view/mod.rs` with shared imports and `render_frame` + `render_to_lines_for_test`:

```rust
//! Chat rendering — each sub-module handles one visual region.

mod banner;
mod input;
mod picker;
mod status;
mod timeline;

use crate::chat::app::ChatApp;
use crate::chat::theme::{build_theme, ChatTheme};
use ratatui_core::{
    layout::Rect,
    terminal::{Frame, Terminal},
    text::{Line, Span},
};
use ratatui_macros::vertical;
use ratatui_widgets::block::Block;

// Re-export for external callers (tests)
pub use self::banner::render_welcome_banner;
pub use self::timeline::render_timeline;

pub fn render_to_lines_for_test(app: &mut ChatApp, width: u16, height: u16) -> Vec<String> {
    // ... (unchanged from current view.rs)
}

pub fn render_frame(frame: &mut Frame, app: &mut ChatApp, tick_ms: u64) {
    // ... (unchanged from current view.rs)
}
```

- [ ] **Step 2: Create `src/chat/view/banner.rs`**

Move `render_welcome_banner()` (lines 81-159 of current view.rs).

- [ ] **Step 3: Create `src/chat/view/timeline.rs`**

Move `render_timeline()`, `render_entry()`, `marker_style_for()`, and constants `MARGIN`/`INDENT` (lines 161-415).

- [ ] **Step 4: Create `src/chat/view/input.rs`**

Move `render_input()` (lines 417-431).

- [ ] **Step 5: Create `src/chat/view/status.rs`**

Move `render_separator()` and `render_status_bar()` (lines 434-458).

- [ ] **Step 6: Create `src/chat/view/picker.rs`**

Move `render_picker()` (lines 460-572).

- [ ] **Step 7: Delete old `src/chat/view.rs`**

Remove the flat file now that view/ directory replaces it.

- [ ] **Step 8: Build and test**

Run: `cargo build && cargo test`
Expected: All tests pass — external imports `blazar::chat::view::{render_frame, render_to_lines_for_test}` are preserved via mod.rs re-exports.

- [ ] **Step 9: Commit**

```bash
git add src/chat/view/ && git rm src/chat/view.rs
git commit -m "refactor(chat): split view.rs into focused rendering modules"
```

---

### Task 4: Add opaline dependency and rewrite theme.rs

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/chat/theme.rs`
- Test: unit test inside `src/chat/theme.rs`

- [ ] **Step 1: Add opaline to Cargo.toml**

```toml
opaline = "0.4"
```

- [ ] **Step 2: Rewrite `src/chat/theme.rs`**

Replace hand-coded Solarized constants with opaline-backed theme loading. Keep `ChatTheme` struct unchanged so all view code compiles without modification. Key changes:

```rust
//! Theme engine — loads opaline themes and maps semantic tokens to ChatTheme.

use opaline::{Theme as OpalineTheme, load_by_name, list_available_themes, ThemeInfo};
use ratatui_core::style::{Color, Modifier, Style};

/// Default theme name.
pub const DEFAULT_THEME: &str = "one-dark";

// Keep SolarizedStyleSheet renamed to ThemeStyleSheet, dynamically configured
pub struct ThemeStyleSheet { /* heading/code/link styles derived from opaline theme */ }

pub struct ChatTheme { /* all fields unchanged */ }

/// Build ChatTheme from an opaline theme by name.
pub fn build_theme() -> ChatTheme {
    build_theme_by_name(DEFAULT_THEME)
}

pub fn build_theme_by_name(name: &str) -> ChatTheme {
    match load_by_name(name) {
        Ok(theme) => map_opaline_to_chat_theme(&theme),
        Err(_) => fallback_theme(),
    }
}

/// Map opaline semantic tokens to ChatTheme fields.
fn map_opaline_to_chat_theme(theme: &OpalineTheme) -> ChatTheme { ... }

/// Hard-coded fallback (current Solarized values).
fn fallback_theme() -> ChatTheme { ... }

/// List all available theme names for the picker.
pub fn available_themes() -> Vec<ThemeInfo> {
    list_available_themes()
}

/// Convert an opaline color to ratatui Color.
fn to_ratatui_color(c: opaline::OpalineColor) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}
```

- [ ] **Step 3: Add unit test for theme loading**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_loads_successfully() {
        let theme = build_theme();
        // Verify non-default styles are set
        assert_ne!(theme.body_text, Style::default());
    }

    #[test]
    fn fallback_works_for_invalid_name() {
        let theme = build_theme_by_name("nonexistent-theme-xyz");
        assert_ne!(theme.body_text, Style::default());
    }

    #[test]
    fn available_themes_returns_nonempty() {
        assert!(!available_themes().is_empty());
    }
}
```

- [ ] **Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: All pass. Theme visually changes to One Dark colors.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/chat/theme.rs
git commit -m "feat(theme): integrate opaline engine with One Dark default"
```

---

### Task 5: Wire runtime theme switching via ChatApp

**Files:**
- Modify: `src/chat/app.rs`
- Modify: `src/chat/view/mod.rs`
- Modify: `src/chat/theme.rs` (ThemeStyleSheet for markdown)

- [ ] **Step 1: Add theme state to ChatApp**

```rust
// In ChatApp struct, add:
theme_name: String,
theme: ChatTheme,
markdown_stylesheet: ThemeStyleSheet,
```

Initialize in `new()`:
```rust
theme_name: crate::chat::theme::DEFAULT_THEME.to_owned(),
theme: crate::chat::theme::build_theme(),
markdown_stylesheet: crate::chat::theme::build_stylesheet(&crate::chat::theme::build_theme()),
```

Add accessors:
```rust
pub fn theme(&self) -> &ChatTheme { &self.theme }
pub fn theme_name(&self) -> &str { &self.theme_name }
pub fn markdown_stylesheet(&self) -> &ThemeStyleSheet { &self.markdown_stylesheet }

pub fn set_theme(&mut self, name: &str) {
    self.theme = crate::chat::theme::build_theme_by_name(name);
    self.markdown_stylesheet = crate::chat::theme::build_stylesheet(&self.theme);
    self.theme_name = name.to_owned();
}
```

- [ ] **Step 2: Update render_frame to use app.theme()**

In `src/chat/view/mod.rs`, replace `let theme = build_theme();` with `let theme = app.theme().clone();`. Remove the `build_theme` import.

Pass theme through all render functions. This is a mechanical find-replace — each render function already takes `&ChatTheme`.

- [ ] **Step 3: Update timeline markdown rendering**

In `src/chat/view/timeline.rs`, replace `SolarizedStyleSheet` usage with `app.markdown_stylesheet()` or pass the stylesheet from render_frame.

- [ ] **Step 4: Add /theme command to picker**

The `/theme` command already exists in the picker command list. Wire it in `handle_action` so that when user selects `/theme`, open a second picker with theme names:

```rust
// In handle_action Submit branch:
if cmd == "/theme" {
    let theme_items: Vec<PickerItem> = crate::chat::theme::available_themes()
        .into_iter()
        .map(|info| PickerItem::new(&info.name, &info.display_name))
        .collect();
    self.picker = ModalPicker::new("Select Theme", theme_items);
    self.picker.open();
    return;
}
```

When a theme name is selected (doesn't start with `/`), apply it:
```rust
if !cmd.starts_with('/') {
    self.set_theme(&cmd);
    // Restore command palette for next time
    self.picker = ModalPicker::command_palette();
    return;
}
```

- [ ] **Step 5: Build and test**

Run: `cargo build && cargo test`
Expected: All pass. `/theme` → select theme → colors change.

- [ ] **Step 6: Commit**

```bash
git add src/chat/app.rs src/chat/view/ src/chat/theme.rs
git commit -m "feat(theme): runtime theme switching via /theme command"
```

---

### Task 6: Update snapshot tests

**Files:**
- Modify: `tests/chat_render_snapshot.rs`
- Modify: `tests/snapshots/` (regenerate)

- [ ] **Step 1: Run snapshot tests to see diffs**

```bash
cargo test chat_render_snapshot -- --nocapture
```

If snapshots fail because colors changed (Solarized → One Dark), review the diffs.

- [ ] **Step 2: Accept new snapshots**

```bash
cargo insta review
```

Or update inline. The visual changes are expected — new default theme.

- [ ] **Step 3: Final full test run**

```bash
just fmt-check && just lint && just test
```

Expected: All green.

- [ ] **Step 4: Commit**

```bash
git add tests/ -A
git commit -m "test: update snapshots for One Dark default theme"
```
