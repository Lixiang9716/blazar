# Users Input Three-Zone Layout Design

## Problem

The current users area in chat view is functionally split, but its rendering responsibilities are still clustered and not aligned with the intended interaction model:

1. top area should show simple workspace context by default
2. middle area should be dedicated to input
3. bottom area should focus on model/runtime metadata

Additionally, when slash-command mode is active, the top area should present commands vertically and support scrolling, instead of a single horizontal status line.

## Goals

1. Refactor users area into explicit three panels: top/middle/bottom.
2. Keep product state in Blazar-owned types (`ChatApp`), not in widget-local state.
3. Add a horizontal separator between middle input panel and bottom model panel.
4. In slash-command mode, render top area as a vertically scrollable command window (max 6 visible commands).
5. Preserve existing interaction behavior outside this scope.

## Non-Goals

1. No redesign of timeline rendering.
2. No command matching algorithm changes.
3. No picker overlay redesign.

## Proposed Architecture

Refactor `src/chat/view/users.rs` into a composition root that renders three explicit panels:

1. `top_panel` — workspace/command context display
2. `input_panel` — user composer and cursor placement
3. `model_panel` — mode/model/context usage display

`users.rs` only performs layout slicing and delegates each panel render to dedicated modules.

## Panel Behavior

### Top Panel

Normal mode (`StatusMode::Normal`):

- show only `path + branch`

Command mode (`StatusMode::CommandList`):

- render command matches as a vertical list
- show at most 6 items in viewport
- support scroll window via Blazar-owned offset state
- top panel receives more available height in users-area distribution

### Input Panel

- preserve current input prompt/composer rendering
- preserve current cursor and IME behavior

### Model Panel

- show execution mode (`AUTO/PLAN`)
- show active model name
- show context usage (`used/max`, percentage when available)
- maintain current semantics and formatting intent

### Separator

- render a horizontal line between input panel and model panel
- separator is presentational only (no new domain state)

## State and Data Flow

Data source remains `ChatApp`.

Add a dedicated users-area command-scroll state field in `ChatApp` (e.g. `users_command_scroll_offset: usize`) and keep command-window logic driven from app state.

Flow:

1. `users.rs` reads snapshot and command-match data from `ChatApp`
2. `top_panel` derives visible command window from `inline_command_matches + users_command_scroll_offset`
3. scroll events in command mode update `users_command_scroll_offset`
4. leaving command mode resets to normal top display (path/branch)

## Interaction Rules

1. If slash command mode is active, scroll input is routed to top-panel command window first.
2. If slash command mode is not active, existing scroll behavior remains unchanged.
3. Command window viewport is capped at 6 visible rows.

## Edge Handling

1. If command list is empty, top panel shows a clear empty-state line.
2. If area height is tight, preserve input and model panels; top panel is clipped first.
3. Scroll offset is clamped to valid range whenever command list changes.

## Testing Plan

1. Add/adjust render assertions for three-zone users area composition.
2. Verify top panel normal mode displays path/branch.
3. Verify command mode displays vertical list with max 6 visible rows.
4. Verify scroll updates visible command window content.
5. Verify mode switch back to normal restores path/branch top display.
6. Verify separator presence and that cursor placement in input panel is unaffected.
7. Run:
   - `just fmt-check`
   - `just lint`
   - `just test`

## Risks and Mitigations

1. **Risk:** Scroll routing conflicts with existing timeline scroll behavior.  
   **Mitigation:** Route scroll conditionally by `StatusMode::CommandList` only.

2. **Risk:** View logic regresses due to panel split.  
   **Mitigation:** Keep state contracts unchanged and add focused render tests.

## Rollout

Ship as a single refactor slice (no feature flag), since behavior change is scoped and testable.
