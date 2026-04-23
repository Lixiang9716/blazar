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

obs-install-tools:
    bash scripts/observability/install-tools.sh

logs-tail log_file="logs/blazar.log":
    bash scripts/observability/logs-tail.sh {{quote(log_file)}}

logs-errors log_file="logs/blazar.log":
    bash scripts/observability/logs-errors.sh {{quote(log_file)}}

logs-turn turn_id log_file="logs/blazar.log":
    bash scripts/observability/logs-turn.sh {{quote(turn_id)}} {{quote(log_file)}}

eval-self-test:
    python3 scripts/eval/open_benchmark_runner.py self-test
    python3 -m unittest tests/eval/test_open_benchmark_runner.py

eval-prepare mode="smoke" datasets="bfcl,toolbench,swebench_lite" full_rows="250":
    python3 scripts/eval/open_benchmark_runner.py prepare --mode {{mode}} --datasets {{datasets}} --full-rows {{full_rows}}

eval-run mode="smoke" datasets="bfcl,toolbench,swebench_lite" max_cases="20":
    python3 scripts/eval/open_benchmark_runner.py run --mode {{mode}} --datasets {{datasets}} --max-cases {{max_cases}}

eval-run-dry mode="smoke" datasets="bfcl,toolbench,swebench_lite" max_cases="20":
    python3 scripts/eval/open_benchmark_runner.py run --mode {{mode}} --datasets {{datasets}} --max-cases {{max_cases}} --dry-run

eval-smoke:
    python3 scripts/eval/open_benchmark_runner.py prepare --mode smoke --datasets bfcl,toolbench,swebench_lite
    python3 scripts/eval/open_benchmark_runner.py run --mode smoke --datasets bfcl,toolbench,swebench_lite --max-cases 10

eval-smoke-dry:
    python3 scripts/eval/open_benchmark_runner.py prepare --mode smoke --datasets bfcl,toolbench,swebench_lite
    python3 scripts/eval/open_benchmark_runner.py run --mode smoke --datasets bfcl,toolbench,swebench_lite --max-cases 10 --dry-run

cov:
    cargo llvm-cov --workspace --all-features --lcov --output-path target/llvm-cov.info

audit:
    cargo deny check

deps:
    cargo outdated -R

snapshots:
    cargo nextest run --test chat_render_snapshot

preflight: fmt-check lint test audit
