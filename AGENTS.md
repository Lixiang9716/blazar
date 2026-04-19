# Blazar Agent Instructions

Before searching the repository or changing code, read:

1. `docs/knowledge-base/blazar-coding-standards.md`
2. `docs/knowledge-base/2026-04-18-ratatui-codex-guide.md`

Treat `blazar-coding-standards.md` as the operational rulebook for implementation work.

## Non-negotiable rules

- Build Blazar as a **Codex-like terminal coding assistant**, not a generic chat shell or dashboard.
- Keep **product state** inside Blazar-owned state types. Do not let widget libraries or rendering helpers become the place where session, workspace, git, task, or permission state lives.
- Prefer improvements that increase **workflow usefulness** over cosmetic polish. Do not spend the main implementation effort on mascot, animation, color, or visual tweaks while workflow/state gaps remain.
- New surfaces must help the user **decide or act**. Avoid read-only decorative panels that do not change the user’s ability to work.
- Follow the shared repository quality gates: `just fmt-check`, `just lint`, `just test`.

If a change conflicts with the coding standards document, follow the coding standards document.
