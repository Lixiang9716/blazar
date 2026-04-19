# Blazar Copilot Instructions

Before making architecture or code decisions in this repository, read:

1. `docs/knowledge-base/blazar-coding-standards.md`
2. `docs/knowledge-base/2026-04-18-ratatui-codex-guide.md`
3. `docs/knowledge-base/generated/graphify/GRAPH_REPORT.md` when deeper repo-specific orientation is needed

## How to use the knowledge base

- Treat `blazar-coding-standards.md` as the **operational rulebook** for daily implementation work.
- Treat `2026-04-18-ratatui-codex-guide.md` as the **architecture and product-direction rationale**.
- Treat `GRAPH_REPORT.md` as supporting evidence for repo structure and graph-level relationships, not as the only guidance source.

## Repository rules

- Build Blazar as a **Codex-like terminal coding assistant**, not as a generic chat TUI or dashboard.
- Keep core product state in Blazar-owned types. Do not move session, workspace, git, task, permission, or knowledge state into rendering helpers or third-party widget state.
- Prefer workflow usefulness, actionability, and safety over mascot polish or purely decorative UI work.
- New surfaces must help the user decide, understand risk, or take action.
- Prefer targeted library adoption over framework takeover.

## Validation

Use the repository quality gates:

- `just fmt-check`
- `just lint`
- `just test`
