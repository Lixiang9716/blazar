# TUI Refactor: Code Splitting + Opaline Theme Engine

**Date:** 2026-04-20
**Status:** Approved

## Problem

After the picker refactor, two large files remain:
- `chat/app.rs` (606 lines) — mixes state, event loop, demo playback
- `chat/view.rs` (572 lines) — all rendering in one flat file

Theme colors are hard-coded Solarized Dark constants with no runtime switching.

## Approach

Three parallel workstreams:

1. **Pure code splitting** — extract logical units into focused modules
2. **Opaline theme engine** — replace hand-coded colors with opaline v0.2
3. **Timeline stays Paragraph** — no scroll mechanism change (user decision)

## 1. Code Splitting

### chat/app.rs → 3 files

| New file | Contents | ~Lines |
|---|---|---|
| `chat/demo.rs` | `demo_timeline()`, `demo_playback_script()` | ~200 |
| `chat/event_loop.rs` | `run_terminal_chat()`, `TerminalGuard`, `resolve_repo_path()` | ~90 |
| `chat/app.rs` (remains) | `ChatApp` struct + impl, `shorten_home()`, `detect_branch()` | ~275 |

### chat/view.rs → view/ module directory

| New file | Contents | ~Lines |
|---|---|---|
| `chat/view/mod.rs` | `render_frame`, `render_to_lines_for_test`, shared imports | ~80 |
| `chat/view/banner.rs` | `render_welcome_banner` | ~80 |
| `chat/view/timeline.rs` | `render_timeline`, `render_entry`, `marker_style_for`, MARGIN/INDENT | ~180 |
| `chat/view/input.rs` | `render_input` | ~20 |
| `chat/view/status.rs` | `render_separator`, `render_status_bar` | ~30 |
| `chat/view/picker.rs` | `render_picker` | ~115 |

### chat/mod.rs update

```rust
pub mod app;
pub mod demo;
pub mod event_loop;
pub mod git;
pub mod input;
pub mod model;
pub mod picker;
pub mod session;
pub mod theme;
pub mod view;
```

## 2. Opaline Theme Engine

### Dependency

```toml
opaline = { version = "0.2", features = ["ratatui"] }
```

### Design Principle

**Blazar owns the theme interface; opaline is a color data source.**

- `ChatTheme` struct stays as the rendering contract — views don't change
- `build_theme(name: &str) -> ChatTheme` loads opaline theme, maps semantic tokens to ChatTheme fields
- Default theme: `"one-dark"` (One Dark from opaline's 39 builtins)
- Fallback: if theme load fails, construct ChatTheme from hard-coded defaults

### Token Mapping

| ChatTheme field | Opaline token/derivation |
|---|---|
| `title_text` | `text.primary` + BOLD |
| `timeline_bg` | `bg.base` as background |
| `body_text` | `text.primary` |
| `dim_text` | `text.muted` |
| `bold_text` | `text.primary` + BOLD |
| `marker_response` | `accent.primary` |
| `marker_tool` | `accent.secondary` or green-ish token |
| `marker_warning` | style `"error_style"` fg |
| `marker_hint` | `accent.primary` |
| `tool_label` | `text.primary` + BOLD |
| `tool_target` | `accent.info` or cyan token |
| `diff_add` | style `"addition"` or green |
| `diff_del` | style `"deletion"` or red |
| `code_block` | `code.plain` |
| `input_prompt` | `accent.info` |
| `status_bar` | `text.secondary` |
| `picker_title` | `accent.primary` + BOLD |
| `picker_selected` | `accent.info` + BOLD |

Exact mapping may vary based on available opaline tokens; fallback to color derivation where needed.

### ThemeStyleSheet (tui-markdown)

Replace `SolarizedStyleSheet` with `ThemeStyleSheet` that reads from the current opaline theme:

```rust
pub struct ThemeStyleSheet {
    heading_1: Style,
    heading_2: Style,
    heading_other: Style,
    code: Style,
    link: Style,
    blockquote: Style,
}
```

Constructed alongside `ChatTheme` from the same opaline theme.

### MascotPalette

Derive mascot colors from opaline theme accent palette instead of hard-coded ANSI escapes.
Keep fallback to current palette if theme lacks sufficient accent colors.

### Runtime Theme Switching

- `ChatApp` gains `theme_name: String` field (default: `"one-dark"`)
- `ChatApp` gains `theme: ChatTheme` field (cached, rebuilt on switch)
- `/theme` command opens a picker listing all available opaline themes
- On selection: update `theme_name`, rebuild `ChatTheme`, picker closes
- `render_frame` reads `app.theme()` instead of calling `build_theme()` each frame

### Theme Persistence (deferred)

Theme preference could be saved to `config/app.json` in the future.
Not in scope for this refactor.

## 3. Timeline (no change)

Timeline rendering stays as `Paragraph` with manual scroll offset.
The code-split moves it to `chat/view/timeline.rs` but logic is unchanged.

## Test Strategy

- Existing tests pass unchanged (API compatibility)
- Add theme construction test: load "one-dark", verify ChatTheme fields are non-default
- Add theme fallback test: invalid name returns sensible defaults
- Snapshot tests may need update if default colors change (Solarized → One Dark)

## Files Changed

### New files
- `src/chat/demo.rs`
- `src/chat/event_loop.rs`
- `src/chat/view/mod.rs`
- `src/chat/view/banner.rs`
- `src/chat/view/timeline.rs`
- `src/chat/view/input.rs`
- `src/chat/view/status.rs`
- `src/chat/view/picker.rs`

### Modified files
- `src/chat/app.rs` — remove extracted code, add theme field
- `src/chat/mod.rs` — add new modules
- `src/chat/theme.rs` — rewrite to use opaline
- `src/welcome/theme.rs` — derive from opaline (optional)
- `Cargo.toml` — add opaline dependency
- `tests/chat_render_snapshot.rs` — update snapshots if colors change

### Deleted files
- `src/chat/view.rs` — replaced by `src/chat/view/` directory
