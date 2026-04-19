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

## Optional local hook

Enable the repository hook path:

```bash
git config core.hooksPath .githooks
```

After that, every commit runs `just preflight`.
