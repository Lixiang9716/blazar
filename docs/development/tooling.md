# Development Tooling

## Required local tools

Install the shared local toolchain:

```bash
cargo install just bacon cargo-nextest cargo-deny cargo-llvm-cov cargo-outdated
```

## Daily commands

- `just fmt` — apply formatting
- `just fmt-check` — verify formatting
- `just lint` — run clippy as the shared lint gate
- `just test` — run the full test suite with nextest
- `just cov` — produce coverage output
- `just audit` — run dependency policy checks
- `just deps` — inspect outdated dependencies
- `just snapshots` — run snapshot-oriented tests
- `just preflight` — run the default local quality gate

## Knowledge-base workflow

Before architecture, UI, or implementation work, read:

1. `docs/knowledge-base/blazar-coding-standards.md`
2. `docs/knowledge-base/2026-04-18-ratatui-codex-guide.md`

Use them differently:

- `blazar-coding-standards.md` is the **operational rulebook** for daily coding and review.
- `2026-04-18-ratatui-codex-guide.md` is the **research-backed rationale** for product direction, architecture choices, and library selection.

Do not treat the knowledge base as passive reference material. Use it to decide:

- whether a change improves workflow usefulness
- whether product state still lives in Blazar-owned types
- whether a new surface is actionable enough to justify its complexity
- whether polish work is being prioritized too early

## Optional local hook

Enable the repository hook path:

```bash
git config core.hooksPath .githooks
```

After that, every commit runs `just preflight`.

## GitHub Actions CI

The repository CI mirrors the shared local quality gate on GitHub Actions.

- Workflow file: `.github/workflows/ci.yml`
- Triggers: pushes to `master` and all pull requests
- Checks: `just fmt-check`, `just lint`, `just test`, `just audit`

If CI fails, reproduce the same command locally through `just` before pushing a fix.
