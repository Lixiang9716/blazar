set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

default:
    @just --list

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo nextest run --all-targets

cov:
    cargo llvm-cov --workspace --all-features --lcov --output-path target/llvm-cov.info

audit:
    cargo deny check

deps:
    cargo outdated -R

snapshots:
    cargo test --test chat_render_snapshot

preflight: fmt-check lint test audit
