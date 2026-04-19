# Picker Refactor: tui-widget-list + tui-overlay

## Problem

The command palette (`ModalPicker`) currently uses hand-written overlay positioning, scroll offset management, and scroll indicators across `picker.rs` (~165 lines) and `view.rs` (`render_picker` ~65 lines). This works but is fragile, lacks animation, has no backdrop dimming, and duplicates behavior that mature ecosystem crates handle better.

## Approach

Replace hand-written overlay and list rendering with two focused crates:

- **`tui-overlay`** — handles modal positioning, backdrop dimming, slide animation, and hit-testing
- **`tui-widget-list`** — handles list scrolling, item selection state, scroll padding, and mouse click support

Blazar retains ownership of command data and filter logic per the coding standard: "Keep product state in Blazar-owned types."

## Architecture

### picker.rs — State Layer

**Remove:**
- `scroll_offset` field
- `visible_window()` method
- `has_scroll_up()` / `has_scroll_down()` methods
- Manual scroll adjustment in `move_up()` / `move_down()`

**Keep:**
- `PickerItem` struct (label + description)
- `ModalPicker` struct as command data owner
- `command_palette()` factory
- `filtered_items()` filter logic
- `push_filter()` / `pop_filter()` filter input
- `select_current()` result extraction
- `PICKER_PAGE_SIZE` constant (no longer used for scroll window, but useful as a soft height hint)

**Add:**
- `tui_widget_list::ListState` field — owns selection index and scroll position
- `tui_overlay::OverlayState` field — owns open/close animation state
- `open()` calls `OverlayState::open()` + resets `ListState`
- `close()` calls `OverlayState::close()`
- `move_up()` delegates to `ListState::previous()`
- `move_down()` delegates to `ListState::next()`
- `selected()` reads from `ListState::selected`
- Remove `selected: usize` field (replaced by `ListState`)
- Remove `visible: bool` field (replaced by `OverlayState`)

### view.rs — Render Layer

**Remove:**
- Entire `render_picker()` function body (~65 lines of manual Rect calculation, Clear, scroll indicators, item rendering)

**Replace with:**
```
1. render Overlay widget (backdrop + chrome) via frame.render_stateful_widget()
2. get inner_area from OverlayState
3. build ListView via ListBuilder (renders each PickerItem as a styled Line)
4. render ListView into inner_area via frame.render_stateful_widget()
5. render footer line below the list
```

Estimated new `render_picker`: ~30 lines.

### app.rs — Event Layer

**Changes:**
- `tick()` calls `overlay_state.tick(elapsed)` each frame for smooth animation
- `move_up/down` in picker mode delegates to `ListState::previous/next`
- Mouse `ScrollUp/Down` in picker mode delegates to `ListState`
- Esc closes picker via `OverlayState::close()` (animated)

### Overlay Configuration

```rust
Overlay::new()
    .anchor(Anchor::BottomLeft)
    .slide(Slide::Bottom)
    .width(Constraint::Length(50))
    .height(Constraint::Length(PICKER_PAGE_SIZE as u16 + 4))
    .backdrop(Backdrop::new(Color::Rgb(0, 0, 0)))
    .block(Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" Commands "))
```

### ListView Configuration

```rust
let builder = ListBuilder::new(|context| {
    let item = &filtered[context.index];
    let style = if context.is_selected { theme.picker_selected } else { theme.picker_item };
    let marker = if context.is_selected { "› " } else { "  " };
    let line = Line::from(vec![
        Span::styled(marker, style),
        Span::styled(&item.label, style),
        Span::styled(format!("  {}", item.description), theme.picker_desc),
    ]);
    (line, 1)  // height = 1 row per item
});

ListView::new(builder, filtered.len())
    .infinite_scrolling(true)
    .scroll_padding(1)
```

## Dependencies

Add to `Cargo.toml`:
```toml
tui-widget-list = "0.15"
tui-overlay = "0.1"
```

## What Does NOT Change

- `PickerItem` struct shape
- `command_palette()` command list
- Filter text input behavior
- `select_current()` return type
- Key bindings (↑↓ navigate, Enter select, Esc cancel, typing filters)
- Footer text content
- Theme colors (`picker_selected`, `picker_item`, `picker_desc`, `picker_title`)

## Testing

- Existing `chat_render` snapshot test continues to pass (picker not visible in default snapshot)
- Manual verification: open picker with `/`, scroll up/down through 22 commands, filter, select, Esc to close with animation
- Verify mouse scroll and click work in picker

## Success Criteria

1. `picker.rs` drops from ~165 lines to ~90 lines
2. `render_picker` in `view.rs` drops from ~65 lines to ~30 lines
3. Picker gains: backdrop dimming, slide animation, mouse click selection, scrollbar
4. All existing behavior preserved: filter, navigate, select, cancel
5. `just fmt-check && just lint && just test` all pass
