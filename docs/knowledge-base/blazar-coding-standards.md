# Blazar Coding Standards

This document turns the knowledge-base research into the **default coding rulebook** for Blazar.

Use it when:

- planning new features
- choosing architecture or widgets
- reviewing a PR or agent change
- deciding whether a UI change is worthwhile

For background and source evidence, see:

- `docs/knowledge-base/2026-04-18-ratatui-codex-guide.md`
- `docs/knowledge-base/generated/graphify/GRAPH_REPORT.md`

## 1. Product identity

Blazar MUST be built as a **Codex-like terminal coding assistant**.

That means code should optimize for:

- repository awareness
- session continuity
- git/workspace operations
- safe action flows
- task and intent visibility

Blazar MUST NOT drift into:

- a generic chat-only TUI
- a dashboard made of passive panels
- an effect-first mascot demo

## 2. State ownership

Blazar MUST own its core product state itself.

Core product state includes:

- workspace selection
- session lifecycle
- git/worktree status
- task / intent / checkpoint state
- confirmation and permission state
- knowledge/context state

Widgets, helpers, and rendering modules MAY display this state, but they MUST NOT become the source of truth for it.

When adding a feature, prefer adding or extending a Blazar state type first, then render it through surfaces. Do not start from a widget and let the widget shape the product model.

## 3. Workflow before polish

Workflow depth MUST come before effect-heavy polish.

Prioritize:

1. state transitions
2. safe operations
3. clear navigation
4. actionable context
5. testable behavior

Defer or minimize:

- animation-first work
- mascot-only layout decisions
- color/theme churn without workflow gain
- decorative panes that do not help the user act

Mascot and theme work is allowed only when it supports comprehension, status, or product identity without displacing useful context.

## 4. Surface design rules

Every major surface MUST answer at least one of these:

- What should I do next?
- What changed?
- What is risky?
- What context do I need before acting?
- What action can I take from here?

If a surface only displays static status and offers no decision support, action path, or context gain, it SHOULD be redesigned or removed.

Read-only surfaces are acceptable only when they materially improve the next decision.

## 5. Knowledge base usage

The knowledge base is not reference wallpaper. It MUST shape implementation choices.

Before architecture or UI work:

1. read this file
2. read the Codex guide
3. use the graph report only as supporting evidence when needed

When writing or reviewing code, ask:

- Does this increase workflow usefulness?
- Does this keep product state in Blazar-owned types?
- Does this add a reusable surface instead of another local rendering hack?
- Does this follow the product direction from the guide?

If the answer is "no", the change is probably off-track.

## 6. Library adoption policy

Prefer small, targeted adoption over framework replacement.

### Adopt now

- `ratatui-textarea` for editor/composer behavior

### Adopt when complexity justifies it

- `tui-overlay` for first-class overlays such as help, command palette, confirmations, or settings
- `tui-widget-list` for noisy list-like surfaces such as sessions, search results, or picker flows

### Avoid as core architecture

- large framework takeover that replaces Blazar’s own runtime/state boundaries too early
- effect libraries used to disguise weak workflow design

## 7. Safety and action design

Git, workspace, and bulk-change actions MUST be safe by default.

Prefer:

- preview before action
- dry-run first behavior
- explicit confirmation for destructive steps
- escalating confirmation for high-risk actions
- clear status after the action completes

Unsafe hidden actions, ambiguous destructive shortcuts, and irreversible flows without confirmation are not acceptable.

## 8. Testing and verification rules

Changes SHOULD be tested at the product-state level, not only at the rendering detail level.

Prefer tests that prove:

- startup decision behavior
- launcher/workspace/session transitions
- git/session/task summaries
- safety behavior for risky actions
- command/key routing

Repository verification commands remain:

- `just fmt-check`
- `just lint`
- `just test`

## 9. Review checklist

Before accepting a change, verify:

1. It strengthens workflow, context, or safety.
2. It does not move product state into rendering helpers or third-party widgets.
3. It does not spend primary effort on polish while workflow gaps remain.
4. It fits the Codex-like product direction.
5. It keeps the repository quality gates green.
