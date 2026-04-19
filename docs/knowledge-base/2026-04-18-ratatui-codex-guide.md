# Ratatui / Codex Project Knowledge Base

## Purpose

This document distills the local `graphify` knowledge-base build and the follow-up recursive research pass into a project-facing guide for Blazar.

For day-to-day implementation rules, use `docs/knowledge-base/blazar-coding-standards.md` as the operational companion to this guide.

Use it as the default reference when making decisions about:

- product direction
- TUI architecture
- session/workspace/git state
- widget and crate selection
- safety, testing, and collaboration patterns

## Source of truth

This guide was synthesized from:

1. A local `graphify` corpus built from `awesome-ratatui` plus key Blazar files
2. A recursive upstream review covering product references, git/workspace tools, and ratatui companion libraries

Generated artifacts copied into the repo:

- `docs/knowledge-base/generated/graphify/GRAPH_REPORT.md`
- `docs/knowledge-base/generated/graphify/graph.html`
- `docs/knowledge-base/generated/graphify/graph.json`

## Executive summary

Blazar should be designed as a **Codex-like terminal coding assistant**, not as a generic chat TUI and not as a framework-first dashboard.

The strongest conclusion from the research is:

1. **Own the core product state yourself**
2. **Adopt ecosystem widgets selectively**
3. **Delay effect-heavy polish until session/workspace/git behavior feels solid**

For a large multi-component TUI, the hard part is not drawing widgets. The hard part is managing:

- session lifecycle
- workspace/folder scope
- git/branch/worktree state
- command/focus/permission state
- safe destructive workflows

## What the graph found in the current code

The local graph of Blazar + `awesome-ratatui` highlighted these current centers of gravity:

- **God nodes**
  - `ChatApp`
  - `render_mascot_lines()`
  - `SpriteAnimation`
- **Key bridges**
  - `TerminalFrame` links mascot rendering to richer image/widget possibilities
  - `Composer TextArea` strongly aligns with `ratatui-textarea`
- **Important question**
  - `Widgets` is the bridge between ecosystem curation and chat runtime, which reinforces that Blazar's next gains come from better surface composition rather than cosmetic tweaks

Interpretation:

- the current code already has good seams around mascot rendering and basic chat runtime
- the next missing layer is product state and reusable surfaces, not another round of local rendering hacks

## Product references and what to borrow

### Oatmeal

Borrow:

- provider/backend abstraction
- editor integration seams
- async streaming
- pragmatic session persistence

Do not copy:

- chat-first product assumptions

### VTCode

Borrow:

- coding-agent command vocabulary
- tool/session/subagent mental model
- provider breadth as a long-term reference

Do not copy:

- protocol-heavy architecture for v1

### OpenCrabs

Borrow:

- SQLite-backed history and usage tracking
- long-running agent product thinking
- fallback/provider resilience as a later-stage reference

Do not copy:

- multi-channel orchestration
- autonomous/RSI complexity

### Tenere

Borrow:

- minimal surface area
- modal/vim-like ergonomics

Do not copy:

- lack of tooling/workspace depth

### claudectl / crmux

Borrow:

- session dashboards
- state propagation models (pull vs push)
- status and cost visibility
- file-conflict and multi-session ideas

Do not copy:

- supervisor-centric product shape as the core UI

## Git / workspace references and what to borrow

### gitui

Borrow:

- staged workflow model
- multi-panel review
- async git operations
- keyboard-first confirmations

### lazyjj

Borrow:

- command transparency
- tabbed context separation
- explicit raw-command escape hatch

Do not copy:

- jj-specific mental model into Git workflows

### Yazi

Borrow:

- async task scheduling
- preview-first navigation
- workspace isolation
- composition with external tools

### Blippy

Borrow:

- permission-aware actions
- review ergonomics
- inline navigation between linked objects
- cached, low-latency workflows

### Deadbranch

Borrow:

- dry-run-first behavior
- protected resource lists
- escalating confirmation levels
- backup/restore mindset

### Repgrep

Borrow:

- preview-before-action
- fine-grained confirmation on bulk edits

## Library recommendations

### Adopt now

#### `ratatui-textarea`

Use as the primary editor/composer primitive.

Why:

- strongest engineering signals in the set
- strong testing discipline
- good fit for code input, prompts, command entry, and multi-line editing

#### `tui-overlay`

Use when overlays become first-class.

Best targets:

- help sheet
- command palette
- settings drawer
- confirmations
- session switcher

#### `tui-widget-list`

Use when list-like surfaces become noisy to maintain by hand.

Best targets:

- session list
- file/search results
- picker-style surfaces
- review/result lists

### Adopt later

#### `tachyonfx`

Only after workflow/state foundations are stable.

Good uses:

- subtle reveals
- loading emphasis
- lightweight transitions

Bad uses:

- effect-first UX
- using motion to mask poor state design

#### `ratatui-interact`

Useful only if Blazar grows into heavy form/dialog/focusable component sets.

#### `opaline`

Useful if Blazar later needs a more formal semantic theme/token system.

### Reference only / avoid for now

#### `rat-salsa`

Keep as an **architecture reference** for large TUI systems.

Use it to study:

- event boundaries
- focus systems
- widget/runtime decomposition

Do not let it replace Blazar's core runtime today.

Reason:

- Blazar's hardest problems are product-state problems
- full framework adoption too early risks fighting your own session/workspace/git model

#### `tui-popup`, `tui-scrollview`

Treat standalone repos as superseded / archived.

Prefer maintained alternatives.

## Architecture recommendation for a large multi-component Blazar TUI

Build Blazar as a **hybrid architecture**:

- core product state is owned by Blazar
- ecosystem widgets are plugged in where they reduce local complexity

### Layer model

#### 1. Shell layer

Owns:

- main layout
- panes
- tabs
- status strip

#### 2. State layer

Owns:

- sessions
- workspace/folder scope
- git state
- task/intent state
- permission and confirmation state

#### 3. Navigation layer

Owns:

- focus
- keymaps
- command palette
- mode transitions

#### 4. Surface layer

Owns:

- timeline
- file tree
- search results
- diffs
- logs
- pickers
- forms

#### 5. Overlay layer

Owns:

- help
- command popups
- confirmations
- drawers
- modal workflows

#### 6. Theme layer

Owns:

- semantic tokens
- status colors
- pane states
- component variants

## Current codebase seam mapping

### `src/chat/app.rs`

Should evolve from basic chat runtime into the owner of:

- session state
- focus state
- app mode
- timeline behavior
- command routing

### `src/chat/view.rs`

Should remain the composition seam for:

- shell layout
- surface composition
- pane rendering
- status surfaces

### `src/chat/input.rs`

Should become the canonical action vocabulary for:

- focus changes
- scroll behavior
- command activation
- overlay open/close

### `src/chat/theme.rs`

Should grow into semantic design tokens for:

- focused/unfocused panes
- status states
- author identity
- overlays
- warnings and destructive actions

### `src/welcome/mascot.rs` and `src/welcome/sprite.rs`

Should stay presentation-only.

These are good seams already and should not become the place where product state leaks in.

## Safety rules

Treat git/workspace mutations as high-risk workflows.

Default rules:

1. show preview first
2. support dry-run where possible
3. keep protected lists for important resources
4. escalate confirmations with risk
5. preserve recovery paths when practical

Examples:

- deleting a branch
- resetting state
- applying generated edits
- mass file operations

## Testing rules

Prefer focused seam tests over giant end-to-end tests.

Priority areas:

- timeline state
- overlay visibility
- confirmation flows
- command routing
- status strip rendering
- keyboard navigation

Patterns worth adopting over time:

- snapshot tests for structured render output
- fuzzing for editor/input behavior
- benchmarks for large lists, timelines, and rendering hot paths

## Collaboration guidance

Blazar does not need full multi-agent orchestration in v1, but it should leave seams for:

- session metadata
- RPC/status updates
- queued mailbox-style messages
- file-conflict visibility
- audit trails for approvals and risky actions

These ideas matter more than a giant dashboard early on.

## Recommended implementation order

1. **Session + workspace + git state foundation**
2. **Status strip + stateful timeline**
3. **Command/help/confirmation overlays**
4. **File/search/review surfaces**
5. **Effects last**

## Decision rule for future additions

Before adding a new crate or subsystem, ask:

1. Does this simplify a real Blazar surface or just add novelty?
2. Does it fit the existing state model without taking ownership away from Blazar?
3. Can the same result be reached with current primitives plus a small local abstraction?
4. Is the workflow problem already clear enough to justify the dependency?

If the answer to these questions is weak, delay adoption.
