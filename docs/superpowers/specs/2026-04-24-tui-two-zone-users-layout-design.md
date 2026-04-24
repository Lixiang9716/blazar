# TUI Two-Zone Layout and Users Region Design

## Problem Statement

Current chat layout is split into multiple stacked regions (`banner`, `timeline`, `streaming`, separators, `input`, `status`) and command discovery relies on an overlay picker panel. This conflicts with the requested interaction model:

1. Top area should be a single **timeline entries** region.
2. Bottom area should be a unified **users** region with three sub-sections.
3. `banner` and `thinking` should be represented as normal timeline entries.
4. Typing `/` should update an inline status command list, not open a separate panel.
5. Mode should be switchable between `auto` and `plan` with `Shift+Tab`.
6. Users region must show path/git/PR/references and model/context usage.

## Goals

1. Collapse chat UI into two primary zones: `timeline` + `users`.
2. Keep product state in `ChatApp` (no state hidden in renderer widgets).
3. Remove command-panel detour for slash flow and make command hints inline.
4. Add explicit mode surface (`auto` / `plan`) with keyboard toggle.
5. Support multiline input with wrapping while preserving fast submit.
6. Surface model/context ratio and workspace/git/PR context in users region.

## Non-Goals

1. Reworking runtime/tool execution protocol.
2. Changing timeline entry semantics outside rendering and grouping.
3. Implementing remote PR APIs; PR display can be local/derived first.
4. Broad theme-system redesign.

## Approaches

### Approach A: Minimal Patch on Existing Panels

- Keep current region stack and picker panel.
- Add incremental tweaks (extra status rows, partial slash behavior).

Trade-off:

- Lowest code churn.
- Keeps core UX mismatch (still panel-based slash flow, banner not timeline-native).

### Approach B: Two-Zone Layout with Inline Users Subregions (Recommended)

- Refactor top-level layout into:
  - `timeline_area` (all entries)
  - `users_area` (status + input + mode/config)
- Migrate `banner` to timeline entry and keep `thinking` as entry type.
- Replace slash panel entrypoint with inline command status mode.

Trade-off:

- Moderate refactor in view + input handling.
- Clear architecture and directly matches requested UX.

### Approach C: Full Unified Entry Engine + Virtualized Footer Widgets

- Move all non-input UI to timeline and make users area highly dynamic/virtualized.
- Build richer command/mode/context widget framework.

Trade-off:

- Most future-proof.
- Too large for this scope and delays delivery.

## Chosen Approach

Adopt **Approach B**.

It satisfies the requested interaction model with bounded change scope, preserves existing runtime state ownership, and minimizes risk versus a full widget framework rewrite.

## Design

### 1. Layout Architecture

Refactor `chat::view::render_frame` to exactly two vertical regions:

1. `timeline_area` (>=1 row)
2. `users_area` (fixed base height `h`, expands when composer wraps/multiline)

Within `users_area`, render three stacked rows:

1. `status_row`
2. `input_row` (multiline-aware; can grow)
3. `mode_row`

Remove standalone banner and streaming strips from top-level layout. Any banner/thinking/streaming cues become timeline entries or status text.

### 2. Timeline Entry Model Changes

Treat banner/thinking as normal timeline entry kinds:

1. Add `EntryKind::Banner` for welcome/system banner content.
2. Keep `EntryKind::Thinking` rendered (no longer globally hidden in timeline renderer).
3. Keep existing message/tool entries unchanged.

Behavior:

1. Session start inserts one `Banner` entry.
2. `has_user_sent` can stop inserting future banners, but existing banner remains a timeline record.
3. Thinking deltas append into the current `Thinking` entry as today.

### 3. Users Region: Status Row

Status row has two display modes:

1. **Normal status mode**
   - Left: `current_path`, `git branch` (if any), `PR name` (if derived/found)
   - Right: referenced files summary (e.g. `refs: src/a.rs, src/b.rs +2`)
2. **Slash command mode** (when composer starts with `/`)
   - Replace normal status text with filtered command list/hints
   - No overlay panel open

State additions in `ChatApp`:

1. `status_mode: StatusMode` (`Normal` / `CommandList`)
2. `referenced_files: Vec<String>`
3. `git_pr_label: Option<String>`
4. `slash_query: String` (derived from composer)
5. `inline_command_matches: Vec<PickerItem>` (reuse registry + filter logic, no modal rendering)

PR source strategy:

1. Phase 1: local heuristic from branch name / merge refs / cached metadata.
2. If unavailable, omit PR label without warning noise.

### 4. Users Region: Input Row

Input row requirements:

1. Prompt prefix is `> `.
2. Supports multiline editing.
3. Wraps visually with available width.

Input semantics:

1. `Enter`: submit current message.
2. `Shift+Enter`: insert newline.
3. Pasted multiline text remains multiline.

Implementation notes:

1. Extend `InputAction` mapping for `Shift+Enter`.
2. Keep using `TextArea`, but allow dynamic input-row height based on composed line count and wrapping.

### 5. Users Region: Mode/Config Row

Mode row shows:

1. Left: mode switcher (`AUTO` / `PLAN`)
2. Right: model name + context usage ratio (`used / max`, plus percentage)

Keyboard:

1. `Shift+Tab` toggles `AUTO <-> PLAN`.

State additions:

1. `user_mode: UserMode` (`Auto`, `Plan`)
2. `context_usage: Option<ContextUsage>` where `ContextUsage { used_tokens, max_tokens }`

Behavior:

1. In `Auto`, submit follows current default chat flow.
2. In `Plan`, submit routes through plan prompt construction (without requiring `/plan` text).

### 6. Slash Command Flow (Inline)

Replace current `/`-opens-picker behavior:

1. If composer begins with `/`, set `status_mode = CommandList`.
2. Filter command registry using typed slash query.
3. Render top matches in status row.
4. Submission executes selected/typed command path through existing command dispatch logic.

Keep modal picker only for contexts explicitly requiring it (e.g., theme/model sub-selection), but slash command discovery is inline-first.

### 7. Data Flow and State Ownership

All new state lives in `ChatApp`:

1. mode state
2. status display state
3. slash filtering state
4. context usage state
5. reference file summary state

Render functions remain pure projections of `ChatApp` state.

### 8. Error Handling

1. If git/PR info unavailable, status degrades gracefully (path only).
2. If context usage unavailable from provider, show `context: n/a`.
3. Slash filtering with zero matches shows deterministic fallback (`No command matches`), not empty row.
4. No silent submit redirection; mode-driven routing is explicit and visible in mode row.

## Testing Plan

Add/adjust tests across view/app/input paths:

1. Layout snapshot tests:
   - two-zone frame structure
   - users region with 3 subrows
2. Timeline rendering tests:
   - banner rendered as entry
   - thinking entry rendered
3. Slash behavior tests:
   - typing `/` does not open modal picker
   - status row switches to command list mode
4. Input tests:
   - `Shift+Enter` inserts newline
   - wrapped multiline rendering keeps prompt alignment
5. Mode tests:
   - `Shift+Tab` toggles mode
   - plan mode submission uses plan route without `/plan` prefix
6. Status row tests:
   - path + branch + optional PR rendering
   - reference files summary rendering
7. Mode row tests:
   - model name + context ratio text formatting

## Implementation Slices

1. **Slice 1**: Two-zone layout scaffolding + users subrows (no behavior change yet).
2. **Slice 2**: Banner/thinking entry rendering model update.
3. **Slice 3**: Inline slash status mode (remove slash-triggered picker open).
4. **Slice 4**: Multiline input semantics + `Shift+Enter`.
5. **Slice 5**: Mode row + `Shift+Tab` + mode-aware submit routing.
6. **Slice 6**: Git/PR/reference/context metadata integration.

## Review Checklist (Self-Validated)

1. No TODO/TBD placeholders.
2. Architecture and behavior are consistent with two-zone requirement.
3. Scope is focused on TUI layout + interaction model, not runtime rewrite.
4. Ambiguous inputs (PR source/context availability) have explicit fallback behavior.
