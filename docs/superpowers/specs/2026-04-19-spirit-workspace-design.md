# Spirit Workspace TUI Redesign

## Problem

The current TUI layout gives the mascot pane too much space and compresses the actual chat workflow. It also has no first-class surface for the two kinds of state that matter during agentic work: repository status and session progress. The result is a UI that has personality, but not yet a strong workspace model.

## Design decision

Redesign the TUI as a **workspace**, not a single chat screen.

The new interface keeps **Chat** as the default home view, but adds two first-class views:

- **Git** — a lightweight repository status view
- **Sessions** — the primary operational view for agent/task progress

The mascot remains part of the product identity, but moves from a large dedicated pane into a compact support role.

## Goals

1. Make chat the main stage of the product.
2. Add a first-class session-management surface for agent workflows.
3. Add a lightweight Git status surface without turning the app into a full Git client.
4. Preserve the Spirit/mascot identity without wasting screen real estate.
5. Keep the layout responsive enough to degrade cleanly on narrow terminals.

## Non-goals

1. Do not build a full Lazygit replacement.
2. Do not implement full session editing or destructive repository operations in the first pass.
3. Do not add mouse-driven interaction requirements.
4. Do not introduce heavy animation or decorative effects that compete with readability.

## Information architecture

The application becomes a four-layer terminal workspace:

1. **Header**
   - Product title: `Blazar · Spirit Workspace`
   - Current repository and branch
   - Current session summary
   - Global state chip such as `Ready`, `Dirty`, or `Running`
2. **Navigation rail**
   - Three top-level destinations:
     - `Chat`
     - `Git`
     - `Sessions`
   - A small mascot/status card below the destinations
3. **Main content area**
   - Changes based on the selected destination
4. **Bottom utility strip**
   - Composer in Chat
   - Command/help hints in all views

## Layout model

### Wide terminals

Use a three-part layout:

- top header row
- middle split:
  - left rail: narrow, fixed-width navigation + mascot/status card
  - right panel: active workspace view
- bottom strip:
  - chat composer when Chat is selected
  - compact help/status strip otherwise

### Narrow terminals

Collapse to a single main column:

- header
- tab row (`Chat / Git / Sessions`)
- active content
- bottom strip

On narrow terminals, the mascot card shrinks to one status line and no longer renders as a large visual block.

## View specifications

### 1. Chat view

The Chat view remains the landing page and should feel like the primary experience.

Structure:

- header stays global
- left rail shows:
  - current Spirit presence
  - compact mascot card
  - quick hints
- right main panel shows:
  - message timeline
  - clearer separation between Spirit, User, and system/status messages
- bottom composer:
  - titled input area such as `Ask Spirit`
  - visible placeholder when empty
  - visible shortcut hints

Changes from current UI:

- Reduce non-chat visual weight.
- Increase the horizontal share of the message feed.
- Make the composer visually stronger than it is now.
- Treat the mascot as orientation and brand, not as the dominant pane.

### 2. Git view

The Git view is intentionally lightweight and read-oriented.

Structure:

- top summary row:
  - branch
  - dirty/clean
  - ahead/behind
  - staged / unstaged counts
- main body split into two vertical sections:
  - changed files list
  - recent commits list
- footer hint row:
  - refresh
  - switch view
  - optional future actions

First-pass scope:

- Show repository state clearly.
- Support selection/highlighting and scrolling if needed.
- Do not attempt full staging, rebasing, conflict resolution, or commit authoring in v1.

### 3. Sessions view

The Sessions view is the operational core of the workspace and should be richer than Git.

Structure:

- top summary cards:
  - current session title or inferred label
  - current cwd/repository
  - active intent
  - todo counts
- middle split:
  - left: checkpoints / recent session items
  - right: details for the selected item
- bottom strip:
  - hints for navigation and returning to chat

The right-side details panel should be able to show:

- current plan file status
- recent checkpoint summary
- recent turns or short history snippets
- ready/in-progress/done todo counts

First-pass scope:

- prioritize visibility into the current session
- optionally list recent sessions later
- avoid trying to become a full log browser on day one

## State model

Add explicit workspace-level state:

- active view: `Chat | Git | Sessions`
- header summary state
- git snapshot state
- session summary state

Keep chat state separate from workspace shell state so Chat remains independently testable.

Recommended split:

- `ChatApp` continues to own chat timeline and composer state
- a higher-level workspace state owns:
  - active tab
  - git summary data
  - session summary data
  - focus region

## Data sources

### Chat

Reuse the existing chat model and composer flow.

### Git

Start with shell-backed read-only queries, normalized into a small internal data model:

- branch name
- dirty status
- ahead/behind
- changed files
- recent commits

### Sessions

Read from the session workspace and SQL todo summary already used by the agent workflow:

- current session folder
- plan file presence
- checkpoint list
- todo counts by status

## Interaction model

Keyboard-first controls:

- `1 / 2 / 3` switch views
- `Tab` cycles focusable regions
- arrow keys or `j/k` move through lists
- `Enter` opens or confirms selection where relevant
- `Esc` returns focus to the primary region
- chat keeps `Enter` for submit when composer is focused

The key requirement is that switching among Chat, Git, and Sessions feels instant and obvious.

## Visual style

Use a cleaner, more modern terminal hierarchy:

- strong header framing
- restrained borders
- limited accent palette
- clearer selected/focused states
- message styles with higher role contrast

Suggested emphasis:

- Spirit messages: darker accented surface
- User messages: lighter contrasting surface
- status/system text: muted surface
- selected navigation item: strongest accent

The mascot should keep color and identity, but occupy a compact card rather than a large empty pane.

## Error and empty states

Define explicit empty states so the UI still feels intentional:

- no changed files → show `Working tree clean`
- no commit history available → show `No recent commits`
- no session metadata → show `No session details available yet`
- no checkpoints → show `No checkpoints recorded`

Do not silently render large blank regions.

## Testing strategy

Preserve snapshot-driven rendering tests and add view-specific render coverage.

Minimum required coverage for the redesign:

1. Chat snapshot updated to the new workspace shell.
2. Git view render test with representative repository summary data.
3. Sessions view render test with representative session/checkpoint/todo data.
4. Narrow-layout render test proving the single-column fallback.
5. Input/focus tests for view switching and composer safety.

## Delivery strategy

Implement in phases:

1. Introduce workspace shell and tab state.
2. Rebuild Chat into the new shell.
3. Add lightweight Git view.
4. Add session-management view.
5. Add responsive fallback and polish.

This preserves a working chat-first experience while expanding toward a true agent workspace.
