# TUI Sprite Animation Design

**Goal:** Add a reusable terminal sprite pipeline that converts a PNG sprite sheet into terminal frames, supports both `ratatui` output and ANSI-string output, and integrates the first animation into the existing welcome renderer using `assets/spirit/slime/slime_idle.png`.

## Scope

- Build a shared sprite conversion module for terminal-friendly animation frames.
- Use `assets/spirit/slime/slime_idle.png` as the first integrated sprite sheet.
- Support two rendering outputs from the same decoded frame data:
  - `ratatui` lines for a standalone component example
  - ANSI string frames for the current `src/welcome/view.rs` rendering path
- Keep the welcome layout and copy structure intact.

## Non-goals

- Do not replace the existing application shell with a full `ratatui` app.
- Do not introduce terminal image protocols such as Kitty, Sixel, or iTerm2 inline images.
- Do not wire `slime_run.png` or `slime_die.png` in the first pass.
- Do not decode the PNG on every render.

## Current project constraints

- `src/welcome/view.rs` currently renders the mascot column as ANSI-colored strings and joins that column with text on the right.
- `src/welcome/mascot.rs` currently stores hand-authored frame data as character grids.
- `src/welcome/state.rs` already tracks welcome presence mode transitions but does not yet expose a dedicated animation frame counter.
- `assets/spirit/slime/` already contains concrete sprite sheets:
  - `slime_idle.png`
  - `slime_run.png`
  - `slime_die.png`

## Architecture

### 1. Shared terminal sprite module

Add a new module at `src/welcome/sprite.rs` with one responsibility: convert a sprite sheet PNG into cached terminal frames.

This module owns:

- image decoding from PNG bytes
- frame slicing across a horizontal sprite sheet
- pixel-pair compaction from two image rows into one terminal row
- terminal frame storage in an intermediate representation
- export methods for ANSI strings and `ratatui` lines

This module does not own welcome-specific copy, column layout, or presence-mode transitions.

### 2. Intermediate frame model

Use a small, UI-library-neutral frame representation:

- `TerminalCell` — one terminal character plus optional foreground/background RGB color
- `TerminalFrame` — a 2D grid of `TerminalCell`
- `SpriteAnimation` — cached `Vec<TerminalFrame>` plus timing state for playback

This keeps the conversion logic independent from the output layer and prevents duplicate conversion code.

### 3. Two rendering outputs

The same `TerminalFrame` should support:

- `to_ratatui_lines() -> Vec<Line<'static>>`
- `to_ansi_string() -> String`

`ratatui` output satisfies the standalone component example.
ANSI output satisfies the current welcome rendering code.

## Sprite conversion rules

The first implementation should treat the selected asset as a horizontal strip where all frames have equal width.

### Frame slicing

- Read the full PNG into RGBA pixels.
- Determine `frame_width = image_width / frame_count`.
- Reject the sprite sheet if the width is not divisible by the configured frame count.

### Pixel compaction

Each pair of vertical pixels becomes one terminal cell:

- transparent + transparent -> `' '` with no color
- color + transparent -> `'▀'` with foreground color from the top pixel
- transparent + color -> `'▄'` with foreground color from the bottom pixel
- same-color top + bottom -> `'█'` with foreground color
- different-color top + bottom -> `'▀'` with top as foreground and bottom as background

Transparency is determined from alpha, with near-transparent pixels treated as empty.

### Color strategy

Use 24-bit RGB colors from the source PNG. Do not quantize to the current mascot palette in the conversion layer.

## Integration into the current project

### `src/welcome/sprite.rs`

Add the reusable conversion and playback types here:

- `Rgb`
- `TerminalCell`
- `TerminalFrame`
- `SpriteAnimation`
- `SpriteError`

Expose a constructor that accepts embedded PNG bytes and a known frame count.

### `src/welcome/mascot.rs`

Keep this file responsible for selecting which mascot content to show for a given `PresenceMode`.

The file should evolve from “stores raw character grids” to “selects animation source and frame policy.” In the first pass, it should keep the existing pose enum and map the welcome mascot to the slime idle sprite animation source.

This lets the project retain a clear place for mascot-specific decisions without forcing `view.rs` to know asset paths.

### `src/welcome/view.rs`

Keep this file responsible for layout and right-column copy only.

Integrate the sprite output by replacing the current manual mascot compaction path with precomputed ANSI lines from the sprite module:

- obtain the selected terminal frame
- convert it to ANSI string
- split it into left-column lines
- keep the existing column-join behavior

The welcome view should not decode images or contain pixel compaction logic after this refactor.

### `src/welcome/state.rs`

Extend the welcome state so the caller can choose a stable animation frame without re-decoding assets.

For the first pass, the state should expose a deterministic frame selector derived from elapsed time, such as:

- compute frame index from `(now_ms - entered_at_ms)` and configured frame interval
- reset naturally on mode transitions because `entered_at_ms` already changes when the mode changes

This preserves the current state model and avoids adding a second mutable counter when elapsed time is already available.

## Asset policy

- First integrated asset: `assets/spirit/slime/slime_idle.png`
- Load it through `include_bytes!` in the mascot/sprite layer for deterministic packaging
- Decode it once and reuse the cached frames

The first pass should not auto-discover all sprite files in `assets/spirit/slime/`. Additional sprite sheets can be added later through explicit wiring.

## Standalone `ratatui` example

The implementation should include a small standalone usage path that demonstrates:

- constructing a `SpriteAnimation` from embedded PNG bytes
- advancing the animation with `tick()`
- rendering the current frame with `Paragraph::new(animation.frame().to_ratatui_lines())`

This example is for reference and API validation; it does not need to replace the current app entrypoint.

## Error handling

The conversion layer should return explicit errors for:

- zero frame count
- zero FPS
- invalid PNG decode
- sprite sheet width not divisible by frame count

There should be no silent fallback to hand-authored mascot frames if the configured sprite fails to load.

## Testing

### Conversion tests

- transparent pixel pairs render as spaces
- same-color vertical pairs render as `█`
- mixed-color vertical pairs render with foreground/background color split
- invalid frame-count and width-divisibility errors are returned correctly

### Integration tests

- loading `assets/spirit/slime/slime_idle.png` yields a non-empty frame list
- ANSI output for the selected frame is non-empty and multi-line
- the welcome scene still contains the existing right-column copy after mascot integration

### Regression boundary

The refactor should preserve:

- current welcome copy strings
- current presence-mode transitions
- current left/right column join behavior

## Recommended file responsibilities after implementation

- `src/welcome/sprite.rs`: sprite sheet decode, frame cache, render exports
- `src/welcome/mascot.rs`: mascot asset choice and frame-selection policy
- `src/welcome/view.rs`: page layout and copy composition
- `src/welcome/state.rs`: time-derived frame selection inputs

## Implementation direction

Use a shared terminal frame model as the single source of truth. From that model, expose both ANSI and `ratatui` views. Integrate only `slime_idle.png` in the first pass, cache decoded frames once, and keep the current welcome screen layout stable while removing pixel-conversion logic from `view.rs`.
