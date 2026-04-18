# TUI Sprite Animation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a reusable PNG-to-terminal sprite pipeline, expose both ANSI and `ratatui` rendering outputs, and integrate `assets/spirit/slime/slime_idle.png` into the existing welcome renderer without replacing the current app shell.

**Architecture:** A new `src/welcome/sprite.rs` module will decode `slime_idle.png` once, slice it into frames, and convert each frame into a terminal-friendly intermediate model. That model will export either ANSI strings for the existing `view.rs` path or `ratatui` lines for a standalone component example. The welcome integration will keep layout and copy composition in `view.rs`, move mascot-specific choices into `mascot.rs`, and derive frame selection from elapsed time in `state.rs`.

**Tech Stack:** Rust 2024, `image`, `ratatui`, Cargo tests, ANSI terminal output

---

### Task 1: Create the shared sprite conversion module

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/welcome/mod.rs`
- Create: `src/welcome/sprite.rs`
- Test: `tests/welcome_sprite.rs`

- [ ] **Step 1: Write the failing test**

Create `tests/welcome_sprite.rs` with:

```rust
use blazar::welcome::sprite::SpriteAnimation;

const SLIME_IDLE_PNG: &[u8] = include_bytes!("../assets/spirit/slime/slime_idle.png");

#[test]
fn slime_idle_sheet_decodes_into_four_terminal_frames() {
    let animation = SpriteAnimation::from_png_bytes(SLIME_IDLE_PNG, 4, 8)
        .expect("slime idle sprite sheet should decode into frames");

    assert_eq!(animation.len(), 4);
    assert!(!animation.frame_by_index(0).to_ansi_string().is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --quiet welcome_sprite`
Expected: FAIL with an unresolved import or missing type error because `blazar::welcome::sprite::SpriteAnimation` does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Update `Cargo.toml`:

```toml
[dependencies]
image = "0.25"
ratatui = "0.28"
schemaui = "0.7.1"
serde_json = "1"
```

Update `src/welcome/mod.rs`:

```rust
pub mod mascot;
pub mod sprite;
pub mod startup;
pub mod state;
pub mod theme;
pub mod view;
```

Create `src/welcome/sprite.rs` with:

```rust
use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    time::{Duration, Instant},
};

use image::{Rgba, RgbaImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCell {
    glyph: char,
    fg: Option<Rgb>,
    bg: Option<Rgb>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalFrame {
    rows: Vec<Vec<TerminalCell>>,
}

impl TerminalFrame {
    pub fn to_ansi_string(&self) -> String {
        let mut out = String::new();

        for (row_index, row) in self.rows.iter().enumerate() {
            for cell in row {
                match (cell.fg, cell.bg) {
                    (None, None) => out.push(' '),
                    (fg, bg) => {
                        if let Some(Rgb(r, g, b)) = fg {
                            out.push_str(&format!("\u{1b}[38;2;{r};{g};{b}m"));
                        }
                        if let Some(Rgb(r, g, b)) = bg {
                            out.push_str(&format!("\u{1b}[48;2;{r};{g};{b}m"));
                        }
                        out.push(cell.glyph);
                        out.push_str("\u{1b}[0m");
                    }
                }
            }

            if row_index + 1 != self.rows.len() {
                out.push('\n');
            }
        }

        out
    }
}

pub struct SpriteAnimation {
    frames: Vec<TerminalFrame>,
    current: usize,
    frame_time: Duration,
    last_tick: Instant,
}

impl SpriteAnimation {
    pub fn from_png_bytes(
        png: &[u8],
        frame_count: u32,
        fps: u16,
    ) -> Result<Self, SpriteError> {
        if frame_count == 0 {
            return Err(SpriteError::InvalidFrameCount);
        }
        if fps == 0 {
            return Err(SpriteError::InvalidFps);
        }

        let sheet = image::load_from_memory(png)?.to_rgba8();
        let width = sheet.width();
        let height = sheet.height();

        if width % frame_count != 0 {
            return Err(SpriteError::WidthNotDivisible { width, frame_count });
        }

        let frame_width = width / frame_count;
        let mut frames = Vec::with_capacity(frame_count as usize);

        for frame_index in 0..frame_count {
            let x_offset = frame_index * frame_width;
            frames.push(build_frame(&sheet, x_offset, frame_width, height));
        }

        Ok(Self {
            frames,
            current: 0,
            frame_time: Duration::from_millis(1000 / u64::from(fps)),
            last_tick: Instant::now(),
        })
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn frame_by_index(&self, index: usize) -> &TerminalFrame {
        &self.frames[index % self.frames.len()]
    }
}

#[derive(Debug)]
pub enum SpriteError {
    InvalidFrameCount,
    InvalidFps,
    WidthNotDivisible { width: u32, frame_count: u32 },
    Decode(image::ImageError),
}

impl Display for SpriteError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFrameCount => write!(f, "frame_count must be greater than 0"),
            Self::InvalidFps => write!(f, "fps must be greater than 0"),
            Self::WidthNotDivisible { width, frame_count } => write!(
                f,
                "sprite sheet width {width} is not divisible by frame count {frame_count}"
            ),
            Self::Decode(err) => write!(f, "failed to decode sprite sheet: {err}"),
        }
    }
}

impl Error for SpriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Decode(err) => Some(err),
            _ => None,
        }
    }
}

impl From<image::ImageError> for SpriteError {
    fn from(value: image::ImageError) -> Self {
        Self::Decode(value)
    }
}

fn build_frame(sheet: &RgbaImage, x_offset: u32, frame_width: u32, frame_height: u32) -> TerminalFrame {
    let mut rows = Vec::new();
    let mut y = 0;

    while y < frame_height {
        let mut row = Vec::with_capacity(frame_width as usize);

        for x in 0..frame_width {
            let top = *sheet.get_pixel(x_offset + x, y);
            let bottom = if y + 1 < frame_height {
                *sheet.get_pixel(x_offset + x, y + 1)
            } else {
                Rgba([0, 0, 0, 0])
            };

            row.push(pixel_pair_to_cell(top, bottom));
        }

        rows.push(row);
        y += 2;
    }

    TerminalFrame { rows }
}

fn pixel_pair_to_cell(top: Rgba<u8>, bottom: Rgba<u8>) -> TerminalCell {
    let top_visible = top[3] >= 16;
    let bottom_visible = bottom[3] >= 16;

    match (top_visible, bottom_visible) {
        (false, false) => TerminalCell { glyph: ' ', fg: None, bg: None },
        (true, false) => TerminalCell {
            glyph: '▀',
            fg: Some(Rgb(top[0], top[1], top[2])),
            bg: None,
        },
        (false, true) => TerminalCell {
            glyph: '▄',
            fg: Some(Rgb(bottom[0], bottom[1], bottom[2])),
            bg: None,
        },
        (true, true) if top[0..3] == bottom[0..3] => TerminalCell {
            glyph: '█',
            fg: Some(Rgb(top[0], top[1], top[2])),
            bg: None,
        },
        (true, true) => TerminalCell {
            glyph: '▀',
            fg: Some(Rgb(top[0], top[1], top[2])),
            bg: Some(Rgb(bottom[0], bottom[1], bottom[2])),
        },
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --quiet welcome_sprite`
Expected: PASS with `slime_idle_sheet_decodes_into_four_terminal_frames`.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/welcome/mod.rs src/welcome/sprite.rs tests/welcome_sprite.rs
git commit -m "feat: add terminal sprite conversion core

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

### Task 2: Add dual-output rendering and conversion edge-case coverage

**Files:**
- Modify: `src/welcome/sprite.rs`
- Modify: `tests/welcome_sprite.rs`
- Test: `src/welcome/sprite.rs`

- [ ] **Step 1: Write the failing tests**

Append to `tests/welcome_sprite.rs`:

```rust
use ratatui::text::Line;

#[test]
fn slime_idle_frame_exports_as_ratatui_lines() {
    let animation = SpriteAnimation::from_png_bytes(SLIME_IDLE_PNG, 4, 8)
        .expect("slime idle sprite sheet should decode into frames");

    let lines: Vec<Line<'static>> = animation.frame_by_index(0).to_ratatui_lines();

    assert!(lines.len() > 1);
}
```

Append to `src/welcome/sprite.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_pair_becomes_space() {
        let cell = pixel_pair_to_cell(Rgba([0, 0, 0, 0]), Rgba([0, 0, 0, 0]));

        assert_eq!(cell.glyph, ' ');
        assert_eq!(cell.fg, None);
        assert_eq!(cell.bg, None);
    }

    #[test]
    fn same_color_pair_becomes_full_block() {
        let cell = pixel_pair_to_cell(Rgba([1, 2, 3, 255]), Rgba([1, 2, 3, 255]));

        assert_eq!(cell.glyph, '█');
        assert_eq!(cell.fg, Some(Rgb(1, 2, 3)));
        assert_eq!(cell.bg, None);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --quiet slime_idle_frame_exports_as_ratatui_lines`
Expected: FAIL because `TerminalFrame::to_ratatui_lines()` does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Update `src/welcome/sprite.rs` imports:

```rust
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
```

Extend `TerminalFrame` and `SpriteAnimation` in `src/welcome/sprite.rs`:

```rust
impl TerminalFrame {
    pub fn to_ratatui_lines(&self) -> Vec<Line<'static>> {
        self.rows
            .iter()
            .map(|row| {
                let spans = row
                    .iter()
                    .map(|cell| {
                        let mut style = Style::default();

                        if let Some(Rgb(r, g, b)) = cell.fg {
                            style = style.fg(Color::Rgb(r, g, b));
                        }
                        if let Some(Rgb(r, g, b)) = cell.bg {
                            style = style.bg(Color::Rgb(r, g, b));
                        }

                        Span::styled(cell.glyph.to_string(), style)
                    })
                    .collect::<Vec<_>>();

                Line::from(spans)
            })
            .collect()
    }
}

impl SpriteAnimation {
    pub fn tick(&mut self) {
        if self.last_tick.elapsed() >= self.frame_time {
            self.current = (self.current + 1) % self.frames.len();
            self.last_tick = Instant::now();
        }
    }

    pub fn frame(&self) -> &TerminalFrame {
        &self.frames[self.current]
    }
}
```

Add a module-level doc example to `src/welcome/sprite.rs`:

```rust
/// ```rust
/// use blazar::welcome::sprite::SpriteAnimation;
/// use ratatui::widgets::Paragraph;
///
/// let animation = SpriteAnimation::from_png_bytes(
///     include_bytes!("../../assets/spirit/slime/slime_idle.png"),
///     4,
///     8,
/// )?;
/// let _widget = Paragraph::new(animation.frame_by_index(0).to_ratatui_lines());
/// # Ok::<(), blazar::welcome::sprite::SpriteError>(())
/// ```
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --quiet welcome_sprite`
Expected: PASS for both ANSI and `ratatui` export tests.

Run: `cargo test --quiet same_color_pair_becomes_full_block`
Expected: PASS for the internal conversion rule test.

- [ ] **Step 5: Commit**

```bash
git add src/welcome/sprite.rs tests/welcome_sprite.rs
git commit -m "feat: add dual terminal sprite render outputs

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

### Task 3: Integrate the cached slime idle animation into the welcome renderer

**Files:**
- Modify: `src/welcome/mascot.rs`
- Modify: `src/welcome/state.rs`
- Modify: `src/welcome/view.rs`
- Modify: `tests/welcome_mascot.rs`
- Modify: `tests/welcome_view.rs`

- [ ] **Step 1: Write the failing tests**

Replace `tests/welcome_mascot.rs` with:

```rust
use blazar::welcome::mascot::render_mascot;
use blazar::welcome::state::WelcomeState;

#[test]
fn slime_idle_mascot_renders_as_ansi_multiline_sprite() {
    let mascot = render_mascot(WelcomeState::new(), 0);

    assert!(mascot.contains('\n'));
    assert!(mascot.contains("\u{1b}[38;2;"));
}

#[test]
fn slime_idle_animation_advances_with_elapsed_time() {
    let first = render_mascot(WelcomeState::new(), 0);
    let later = render_mascot(WelcomeState::new(), 260);

    assert_ne!(first, later);
}
```

Update `tests/welcome_view.rs` to:

```rust
use blazar::welcome::state::WelcomeState;
use blazar::welcome::view::render_scene;

#[test]
fn welcome_scene_contains_brand_copy_and_prompt() {
    let scene = render_scene(WelcomeState::new(), 0);

    assert!(scene.contains("BLAZAR"));
    assert!(scene.contains("Star Sugar Guidepony / 星糖导航马"));
    assert!(scene.contains("A rainbow helper just spotted you"));
    assert!(scene.contains("Describe a task to begin"));
}

#[test]
fn welcome_scene_keeps_sprite_and_copy_columns_together() {
    let scene = render_scene(WelcomeState::new(), 0);

    assert!(scene.lines().count() >= 6);
    assert!(scene.contains("> "));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --quiet welcome_mascot`
Expected: FAIL because `render_mascot` does not exist yet.

Run: `cargo test --quiet welcome_view`
Expected: FAIL because `render_scene` does not yet accept `now_ms`.

- [ ] **Step 3: Write minimal implementation**

Replace `src/welcome/mascot.rs` with:

```rust
use std::sync::OnceLock;

use crate::welcome::sprite::SpriteAnimation;
use crate::welcome::state::WelcomeState;

const SLIME_IDLE_PNG: &[u8] = include_bytes!("../../assets/spirit/slime/slime_idle.png");
const SLIME_IDLE_FRAMES: u32 = 4;
const SLIME_IDLE_FPS: u16 = 8;

pub fn render_mascot(state: WelcomeState, now_ms: u64) -> String {
    let animation = slime_idle_animation();
    let frame_index = state.animation_frame_index(now_ms, animation.len(), 125);

    animation.frame_by_index(frame_index).to_ansi_string()
}

fn slime_idle_animation() -> &'static SpriteAnimation {
    static ANIMATION: OnceLock<SpriteAnimation> = OnceLock::new();

    ANIMATION.get_or_init(|| {
        SpriteAnimation::from_png_bytes(SLIME_IDLE_PNG, SLIME_IDLE_FRAMES, SLIME_IDLE_FPS)
            .expect("slime idle sprite should decode")
    })
}
```

Extend `src/welcome/state.rs`:

```rust
impl WelcomeState {
    pub fn animation_frame_index(
        self,
        now_ms: u64,
        frame_count: usize,
        frame_interval_ms: u64,
    ) -> usize {
        let elapsed = now_ms.saturating_sub(self.entered_at_ms);
        ((elapsed / frame_interval_ms) as usize) % frame_count
    }
}
```

Update `src/welcome/view.rs`:

```rust
use crate::welcome::mascot::render_mascot;
use crate::welcome::state::{PresenceMode, WelcomeState};
use crate::welcome::theme::{MASCOT_ALIAS_ZH, MASCOT_NAME, MASCOT_PALETTE, paint};

pub fn render_scene(state: WelcomeState, now_ms: u64) -> String {
    let left = render_mascot(state, now_ms)
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let left_raw_width = left
        .iter()
        .map(|line| strip_ansi(line).chars().count())
        .max()
        .unwrap_or(0);

    let right = vec![
        paint("BLAZAR", MASCOT_PALETTE.blue_ansi),
        format!("{MASCOT_NAME} / {MASCOT_ALIAS_ZH}"),
        status_copy(state.mode()).to_string(),
        String::new(),
        "Describe a task to begin".to_string(),
        "> ".to_string(),
    ];

    join_columns(&left, left_raw_width + 4, left_raw_width, &right)
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.next_if_eq(&'[').is_some() {
            while let Some(next) = chars.next() {
                if next == 'm' {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }

    out
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --quiet welcome_mascot`
Expected: PASS with sprite-backed mascot tests green.

Run: `cargo test --quiet welcome_view`
Expected: PASS with the updated `render_scene` signature and layout assertions.

Run: `cargo test --quiet`
Expected: PASS for the full suite.

- [ ] **Step 5: Commit**

```bash
git add src/welcome/mascot.rs src/welcome/state.rs src/welcome/view.rs tests/welcome_mascot.rs tests/welcome_view.rs
git commit -m "feat: render welcome mascot from sprite sheet

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```
