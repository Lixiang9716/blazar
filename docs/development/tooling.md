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
- `just eval-self-test` — validate benchmark harness logic
- `just eval-prepare smoke bfcl,toolbench,swebench_lite 250` — pull benchmark samples
- `just eval-run smoke bfcl,toolbench,swebench_lite 20` — run model benchmark
- `just eval-run-dry smoke bfcl,toolbench,swebench_lite 20` — benchmark pipeline dry-run without provider calls
- `just eval-smoke` — one-command smoke benchmark
- `just eval-smoke-dry` — one-command smoke dry-run

## Cargo deny duplicate policy

`just audit` includes `cargo deny check` with duplicate-crate warnings enabled.

Current hygiene status:

- `indexmap` is pinned to `2.12.0` in `Cargo.lock` to avoid pulling `hashbrown 0.17.x` via `toml_edit`, reducing one duplicate set.
- A small set of transitive duplicates remains explicitly documented in `deny.toml` (`[bans].skip`) because upstream crates currently require incompatible major lines:
  - `getrandom 0.2.x` (via `ring`/`rustls`) alongside `0.3.x`
  - `unicode-width 0.1.x` (via `termimad`/`ratskin`) alongside `0.2.x`
  - `windows-sys 0.52.x` (via `ring`) alongside `0.61.x`

When upstream dependencies converge, remove those skip entries and re-run `just audit`.

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

## Open benchmark automation

Blazar includes a benchmark harness at `scripts/eval/open_benchmark_runner.py` with default deployment for:

- BFCL (`gorilla-llm/Berkeley-Function-Calling-Leaderboard`)
- ToolBench (`Maurus/ToolBench`)
- SWE-bench Lite (`princeton-nlp/SWE-bench_Lite`)

Typical local flow:

```bash
just eval-prepare smoke bfcl,toolbench,swebench_lite 250
BLAZAR_EVAL_API_KEY=... just eval-run smoke bfcl,toolbench,swebench_lite 10
```

Artifacts:

- Prepared datasets: `target/evals/datasets/<dataset>/<mode>.jsonl`
- Predictions: `target/evals/reports/<dataset>_<mode>_predictions.jsonl`
- Summary report: `target/evals/reports/report_<mode>.json`

GitHub Actions workflow: `.github/workflows/benchmark-evals.yml` (manual trigger).
