# Workspace Launcher Design

**Goal:** Add a lightweight startup layer for choosing or resuming a workspace before entering the existing Spirit Workspace shell.

## Problem

The current Spirit Workspace assumes the user is already inside the correct repository and session context. That works well for a single-repo flow, but it leaves a gap once the product needs to support multiple workspaces:

- there is no explicit place to decide **which workspace** to enter
- the existing **Sessions** view is about activity *inside* the current workspace, not about switching across workspaces
- forcing a full chooser on every launch would add friction for the common "just reopen the last thing" case

The product needs a startup experience that makes workspace selection clear without slowing down the default chat-first workflow.

## Design decision

Introduce a **conditional Workspace Launcher**.

Blazar should:

1. **resume the last workspace by default** when that path is still valid and the user did not explicitly request switching
2. **show the Workspace Launcher** only when selection is actually needed:
   - first run / no saved workspace
   - more than one likely candidate and the user requests switching
   - the last workspace cannot be restored
   - the user launches into an explicit chooser flow
3. keep the existing **Spirit Workspace** as the post-entry shell, with `Chat / Git / Sessions` unchanged in responsibility

This keeps startup fast for the common case while still creating a clear home for cross-workspace navigation.

## Goals

1. Make workspace selection explicit without turning every launch into an extra step.
2. Keep **workspace selection** separate from **session inspection inside a workspace**.
3. Preserve the existing chat-first Spirit Workspace after entry.
4. Make launcher state readable enough that users can decide where to resume without opening each workspace blindly.
5. Support both wide and narrow terminal layouts.

## Non-goals

1. Do not build a full IDE-style project manager with deep project configuration.
2. Do not merge the launcher with the in-workspace **Sessions** view.
3. Do not add destructive repository operations at launch time.
4. Do not require mouse interaction.

## Core concepts

### Workspace

A **workspace** is the top-level working context:

- repository path
- repo summary (branch / clean vs dirty)
- associated session-state directory
- recent activity summary

The launcher chooses **which workspace** to enter.

### Session

A **session** is work history *inside* the currently selected workspace:

- current intent
- checkpoints
- todo counts
- recent timeline or summary

The **Sessions** view continues to answer: "What has been happening in this workspace?"

The launcher should not try to become a global session browser in v1.

## Startup flow

### Normal launch

1. Resolve the most recent valid workspace.
2. If there is a clear workspace to resume and no explicit switch request, enter Spirit Workspace directly.
3. If there is no clear restore target, show the Workspace Launcher.

### Explicit launcher entry

The launcher should also be reachable intentionally:

- a startup flag or dedicated command
- a future in-app "switch workspace" action
- a recovery path when the stored workspace cannot be restored

### Deep-link entry actions

When the launcher is visible, the selected workspace can be opened in different entry modes:

- `Enter` → open the workspace normally into `Chat`
- `S` → open directly into `Sessions`
- `G` → open directly into `Git`

This keeps the launcher useful without making it heavy.

## Information architecture

The application becomes a **two-layer model**:

1. **Launcher layer**
   - decide which workspace to enter
   - expose recent workspace summaries
   - provide quick entry actions
2. **Workspace layer**
   - existing Spirit Workspace shell
   - `Chat`, `Git`, and `Sessions`

The boundary is important:

- launcher = cross-workspace navigation
- sessions view = current-workspace operational history

## Launcher layout

### Wide terminals

Use a two-column launcher:

1. **Left column: recent workspaces**
   - display name
   - repo path
   - branch / clean-dirty state
   - small badges like `active session`, `ready todos`, or `checkpoint count`
   - highlighted current selection
2. **Right column: preview + quick actions**
   - workspace name
   - last intent
   - recent checkpoint
   - session/todo summary
   - actions: `Resume`, `Open Sessions`, `Open Git`
   - a **small Spirit status card** that keeps the mascot visible without overtaking the launcher
3. **Footer**
   - keyboard hints only

The Spirit card should use the same slime identity as the welcome screen, but in a compact footprint. It belongs in the preview/action column, not as a separate hero panel.

### Narrow terminals

Collapse to a single-column stack:

1. launcher title
2. selected workspace card
3. additional workspace rows
4. open/import action
5. compact footer hints

The preview panel is reduced to the most important summary lines:

- repo state
- active session count or last session label
- recent checkpoint / last intent

On narrow terminals, Spirit still appears, but as a **compact animated slime card** or a single stacked status block rather than a large decorative region.

## Visual style

Follow the existing Spirit Workspace shell language:

- default theme: **One Dark Pro**
- dark terminal background
- restrained One Dark Pro accents: blue, green, yellow, red, purple, and cyan used semantically
- selected card with the strongest highlight
- soft borders instead of dense separators
- compact, readable metadata chips instead of verbose prose

The launcher should feel like the **front door** to the same product, not a different tool.

### Theme management

The theme system should move away from hardcoded palette values in `theme.rs` alone.

Use a small user-facing config file, for example:

- `config/theme.json`

That config should own:

- active theme key (default: `one-dark-pro`)
- theme palette tokens
- density / spacing preference
- label style preferences such as uppercase vs sentence case

It should **not** try to own the terminal font family itself. Terminal font selection remains outside the TUI; the application only controls styling tokens and information density.

### Spirit behavior

Spirit should remain visible at startup.

The launcher should use the existing **welcome slime** identity as a **compact animated version**:

- same slime asset family as welcome
- same idle animation language
- smaller presentation suitable for a launcher status card
- no replacement mascot and no static placeholder face

This keeps continuity between welcome, launcher, and workspace surfaces.

## Post-entry workspace behavior

Once a workspace is entered:

- the current Spirit Workspace shell remains the main experience
- `Chat` stays the default landing view
- `Git` and `Sessions` remain top-level in-workspace views
- the user should not see launcher concerns mixed into the `Sessions` panel

The `Sessions` view may later include a small breadcrumb or workspace label, but it should remain scoped to the current workspace.

## Data model

Each saved workspace summary should be able to provide:

- display name
- absolute path
- branch
- clean / dirty state
- recent session label or active session count
- last intent
- latest checkpoint title
- todo counts summary if available
- last opened timestamp

This metadata should be lightweight and safe to load at startup.

## Interaction model

Launcher controls should stay keyboard-first:

- `↑/↓` or `j/k` move selection
- `Tab` cycles focus between list, preview, and quick actions
- `Enter` resumes the selected workspace into `Chat`
- `S` opens the selected workspace in `Sessions`
- `G` opens the selected workspace in `Git`
- `Ctrl+O` opens or imports another workspace
- `Esc` quits

If the launcher is skipped because the last workspace is resumed automatically, these controls are irrelevant until the user explicitly opens the launcher later.

## Empty and failure states

Define explicit startup states:

- **no saved workspaces** → show first-run launcher with `Open workspace`
- **last workspace missing** → show launcher with a restore warning
- **workspace metadata unavailable** → still show the workspace row, but mark preview fields as unavailable
- **no session summary** → preview should show a short neutral fallback such as `No session details yet`

The launcher should never render as a blank shell.

## Testing

Minimum implementation coverage should include:

1. startup flow tests for:
   - restore last workspace
   - launcher shown on first run
   - launcher shown when restore target is invalid
2. render tests for:
   - wide launcher layout
   - narrow launcher layout
   - selected workspace highlight
   - empty state / restore warning state
3. interaction tests for:
   - list navigation
   - quick entry actions (`Enter`, `S`, `G`)
   - launcher-to-workspace transition
4. persistence tests for remembering and restoring the last workspace

## Recommended scope for v1

Ship the smallest complete version:

1. launcher with recent workspace list
2. preview panel with lightweight summary
3. `Resume`, `Sessions`, and `Git` entry actions
4. restore-last-workspace behavior
5. narrow-layout fallback

Leave these for later:

- workspace creation wizards
- global search across all sessions
- launcher-side editing or cleanup of workspace entries
- complex multi-pane project administration
