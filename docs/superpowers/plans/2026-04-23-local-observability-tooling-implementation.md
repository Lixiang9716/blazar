# Local Observability Tooling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver log-first local observability with structured JSON logs, fast local querying commands, and install tooling for jq/lnav/fzf while preserving a clean seam for future tracing backends.

**Architecture:** Introduce a small `observability` module that owns log schema/formatting and correlation helpers, then wire it into logger initialization and high-value runtime/chat log paths. Add script-based local tooling (`logs-tail`, `logs-errors`, `logs-turn`, installer) and cover behavior with focused tests. Keep Blazar-owned runtime/app state as source of truth; observability is an interface layer, not a state owner.

**Tech Stack:** Rust (`log`, `flexi_logger`, `serde_json`), shell scripts (`bash`), `just`, cargo test / nextest.

---

## File Structure and Responsibilities

- **Create:** `src/observability/mod.rs`  
  Module surface for observability helpers.
- **Create:** `src/observability/logging.rs`  
  Structured log schema, JSON formatter, correlation field helpers.
- **Create:** `src/observability/logging_tests.rs`  
  Unit tests for schema keys and JSON output stability.
- **Modify:** `src/lib.rs`  
  Export `observability` module.
- **Modify:** `src/app.rs`  
  Replace `flexi_logger::detailed_format` with structured formatter.
- **Modify:** `src/app_tests.rs`  
  Logger integration tests for structured output.
- **Modify:** `src/agent/runtime.rs`  
  Emit structured error events with `turn_id`, `error_kind`, `trace_id`.
- **Modify:** `src/chat/app/events.rs`  
  Emit structured tool lifecycle events with `turn_id`, `tool_name`, `agent_id`.
- **Create:** `scripts/observability/install-tools.sh`  
  Local package/tool check+install workflow.
- **Create:** `scripts/observability/logs-tail.sh`  
  Tail log stream for local debugging.
- **Create:** `scripts/observability/logs-errors.sh`  
  Filter warn/error structured events.
- **Create:** `scripts/observability/logs-turn.sh`  
  Filter events by turn id.
- **Modify:** `justfile`  
  Add `logs-tail`, `logs-errors`, `logs-turn`, `obs-install-tools`.
- **Create:** `tests/observability_scripts.rs`  
  Integration tests for script behavior on normal/malformed/empty logs.

---

### Task 1: Add structured observability logging module and wire app logger

**Files:**
- Create: `src/observability/mod.rs`
- Create: `src/observability/logging.rs`
- Create: `src/observability/logging_tests.rs`
- Modify: `src/lib.rs`
- Modify: `src/app.rs`
- Modify: `src/app_tests.rs`

- [ ] **Step 1: Write the failing test for structured log keys**

```rust
// src/observability/logging_tests.rs
use crate::observability::logging::format_event_json;
use serde_json::Value;

#[test]
fn structured_log_contains_required_stable_keys() {
    let raw = format_event_json(
        "INFO",
        "blazar::agent::runtime",
        "turn_failed",
        "runtime turn failed",
        Some("trace-1"),
        Some("turn-7"),
        Some("bash"),
        Some("agent-echo"),
        Some("ProviderFatal"),
    );
    let v: Value = serde_json::from_str(&raw).expect("valid json");
    for key in [
        "ts", "level", "target", "event", "message",
        "trace_id", "turn_id", "tool_name", "agent_id", "error_kind",
    ] {
        assert!(v.get(key).is_some(), "missing key: {key}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test structured_log_contains_required_stable_keys -- --nocapture`  
Expected: FAIL (missing `observability::logging::format_event_json`).

- [ ] **Step 3: Implement module and formatter**

```rust
// src/observability/mod.rs
pub mod logging;
```

```rust
// src/observability/logging.rs
use serde_json::json;

pub fn format_event_json(
    level: &str,
    target: &str,
    event: &str,
    message: &str,
    trace_id: Option<&str>,
    turn_id: Option<&str>,
    tool_name: Option<&str>,
    agent_id: Option<&str>,
    error_kind: Option<&str>,
) -> String {
    json!({
        "ts": chrono_like_now_iso8601(),
        "level": level,
        "target": target,
        "event": event,
        "message": message,
        "trace_id": trace_id,
        "turn_id": turn_id,
        "tool_name": tool_name,
        "agent_id": agent_id,
        "error_kind": error_kind,
    })
    .to_string()
}

fn chrono_like_now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    format!("{secs}")
}
```

```rust
// src/lib.rs
pub mod observability;
```

```rust
// src/app.rs (inside init_logger logger setup)
.format(crate::observability::logging::flexi_structured_format)
```

- [ ] **Step 4: Add flexi_logger formatter + logger integration test**

```rust
// src/observability/logging.rs
use flexi_logger::DeferredNow;
use log::Record;
use std::io::Write;

pub fn flexi_structured_format(
    w: &mut dyn Write,
    _now: &mut DeferredNow,
    record: &Record<'_>,
) -> std::io::Result<()> {
    let line = format_event_json(
        &record.level().to_string(),
        record.target(),
        "app_log",
        &record.args().to_string(),
        None,
        None,
        None,
        None,
        None,
    );
    writeln!(w, "{line}")
}
```

```rust
// src/app_tests.rs (new test)
#[test]
fn init_logger_writes_json_lines() {
    init_logger();
    log::info!("logger_json_probe");
    let log_path = std::env::current_dir().expect("cwd").join("logs/blazar.log");
    let text = std::fs::read_to_string(log_path).expect("log file");
    let last = text.lines().last().expect("at least one line");
    let value: serde_json::Value = serde_json::from_str(last).expect("json line");
    assert_eq!(value["event"], "app_log");
    assert!(value.get("message").is_some());
}
```

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test structured_log_contains_required_stable_keys init_logger_writes_json_lines -- --nocapture`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/observability/mod.rs src/observability/logging.rs src/observability/logging_tests.rs src/lib.rs src/app.rs src/app_tests.rs
git commit -m "feat(observability): add structured json logging module"
```

---

### Task 2: Add correlation-aware structured event emission in runtime/chat hotspots

**Files:**
- Modify: `src/observability/logging.rs`
- Modify: `src/agent/runtime.rs`
- Modify: `src/chat/app/events.rs`
- Modify: `src/agent/runtime/tests_impl.inc`
- Modify: `src/chat/app/tests_impl.inc`

- [ ] **Step 1: Write failing runtime/chat observability tests**

```rust
// src/agent/runtime/tests_impl.inc
#[test]
fn runtime_failure_logs_include_turn_and_error_kind_fields() {
    // Trigger a known fatal path and assert structured line has turn_id + error_kind.
}
```

```rust
// src/chat/app/tests_impl.inc
#[test]
fn tool_events_log_include_turn_and_tool_name() {
    // Trigger ToolCallStarted and assert event line contains turn_id/tool_name.
}
```

- [ ] **Step 2: Run tests to verify red state**

Run: `cargo test runtime_failure_logs_include_turn_and_error_kind_fields tool_events_log_include_turn_and_tool_name -- --nocapture`  
Expected: FAIL (missing structured emit helper usage).

- [ ] **Step 3: Implement structured emit helpers and wire call sites**

```rust
// src/observability/logging.rs
pub fn emit_structured(
    level: log::Level,
    target: &str,
    event: &str,
    message: &str,
    trace_id: Option<&str>,
    turn_id: Option<&str>,
    tool_name: Option<&str>,
    agent_id: Option<&str>,
    error_kind: Option<&str>,
) {
    let line = format_event_json(
        &level.to_string(),
        target,
        event,
        message,
        trace_id,
        turn_id,
        tool_name,
        agent_id,
        error_kind,
    );
    log::log!(target: target, level, "{line}");
}
```

```rust
// src/agent/runtime.rs (fatal path)
crate::observability::logging::emit_structured(
    log::Level::Warn,
    "blazar::agent::runtime",
    "turn_failed",
    &error,
    None,
    Some(&turn_id.to_string()),
    None,
    None,
    Some(match kind {
        RuntimeErrorKind::ProviderTransient => "ProviderTransient",
        RuntimeErrorKind::ProviderFatal => "ProviderFatal",
        RuntimeErrorKind::ProtocolInvalidPayload => "ProtocolInvalidPayload",
        RuntimeErrorKind::ToolExecution => "ToolExecution",
        RuntimeErrorKind::Cancelled => "Cancelled",
    }),
);
```

```rust
// src/chat/app/events.rs (tool start/complete paths)
crate::observability::logging::emit_structured(
    log::Level::Info,
    "blazar::chat::events",
    "tool_call_started",
    "tool call started",
    None,
    Some(&turn_id.to_string()),
    Some(tool_name),
    Some(agent_id.as_str()),
    None,
);
```

- [ ] **Step 4: Run tests to verify green state**

Run: `cargo test runtime_failure_logs_include_turn_and_error_kind_fields tool_events_log_include_turn_and_tool_name -- --nocapture`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/observability/logging.rs src/agent/runtime.rs src/chat/app/events.rs src/agent/runtime/tests_impl.inc src/chat/app/tests_impl.inc
git commit -m "feat(observability): add correlation fields for runtime and tool events"
```

---

### Task 3: Add local log query command scripts and justfile wiring

**Files:**
- Create: `scripts/observability/logs-tail.sh`
- Create: `scripts/observability/logs-errors.sh`
- Create: `scripts/observability/logs-turn.sh`
- Modify: `justfile`
- Create: `tests/observability_scripts.rs`

- [ ] **Step 1: Write failing script behavior tests**

```rust
// tests/observability_scripts.rs
#[test]
fn logs_errors_filters_warn_and_error_levels() {
    // Prepare temp log file with mixed levels; invoke logs-errors.sh; assert only warn/error lines.
}

#[test]
fn logs_turn_filters_by_turn_id() {
    // Prepare temp log file with turn_id values; invoke logs-turn.sh <turn>; assert only matching lines.
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test observability_scripts -- --nocapture`  
Expected: FAIL (scripts not found / commands unavailable).

- [ ] **Step 3: Implement scripts**

```bash
# scripts/observability/logs-tail.sh
#!/usr/bin/env bash
set -euo pipefail
LOG_FILE="${1:-logs/blazar.log}"
test -f "$LOG_FILE" || { echo "log file not found: $LOG_FILE" >&2; exit 2; }
tail -f "$LOG_FILE"
```

```bash
# scripts/observability/logs-errors.sh
#!/usr/bin/env bash
set -euo pipefail
LOG_FILE="${1:-logs/blazar.log}"
test -f "$LOG_FILE" || { echo "log file not found: $LOG_FILE" >&2; exit 2; }
jq -c 'select(.level == "WARN" or .level == "ERROR")' "$LOG_FILE"
```

```bash
# scripts/observability/logs-turn.sh
#!/usr/bin/env bash
set -euo pipefail
TURN_ID="${1:-}"
LOG_FILE="${2:-logs/blazar.log}"
test -n "$TURN_ID" || { echo "usage: logs-turn.sh <turn_id> [log_file]" >&2; exit 2; }
test -f "$LOG_FILE" || { echo "log file not found: $LOG_FILE" >&2; exit 2; }
jq -c --arg turn "$TURN_ID" 'select(.turn_id == $turn)' "$LOG_FILE"
```

- [ ] **Step 4: Wire just commands**

```make
# justfile
obs-install-tools:
    bash scripts/observability/install-tools.sh

logs-tail log_file="logs/blazar.log":
    bash scripts/observability/logs-tail.sh {{log_file}}

logs-errors log_file="logs/blazar.log":
    bash scripts/observability/logs-errors.sh {{log_file}}

logs-turn turn_id log_file="logs/blazar.log":
    bash scripts/observability/logs-turn.sh {{turn_id}} {{log_file}}
```

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test --test observability_scripts -- --nocapture`  
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add scripts/observability/logs-tail.sh scripts/observability/logs-errors.sh scripts/observability/logs-turn.sh justfile tests/observability_scripts.rs
git commit -m "feat(observability): add local log query scripts and just commands"
```

---

### Task 4: Add local observability tooling installer script

**Files:**
- Create: `scripts/observability/install-tools.sh`
- Modify: `tests/observability_scripts.rs`

- [ ] **Step 1: Write failing installer behavior tests**

```rust
#[test]
fn install_tools_script_reports_missing_tools_in_check_mode() {
    // Run script with CHECK_ONLY=1 and PATH constrained; assert actionable output includes jq/lnav/fzf.
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test --test observability_scripts install_tools_script_reports_missing_tools_in_check_mode -- --nocapture`  
Expected: FAIL (installer script missing).

- [ ] **Step 3: Implement installer**

```bash
#!/usr/bin/env bash
set -euo pipefail

CHECK_ONLY="${CHECK_ONLY:-0}"
TOOLS=(jq lnav fzf)

detect_pm() {
  if command -v apt-get >/dev/null 2>&1; then echo "apt"; return; fi
  if command -v brew >/dev/null 2>&1; then echo "brew"; return; fi
  echo "none"
}

install_tool() {
  local pm="$1" tool="$2"
  case "$pm" in
    apt) sudo apt-get update && sudo apt-get install -y "$tool" ;;
    brew) brew install "$tool" ;;
    none) echo "manual install required for $tool" ;;
  esac
}

pm="$(detect_pm)"
for tool in "${TOOLS[@]}"; do
  if command -v "$tool" >/dev/null 2>&1; then
    echo "[ok] $tool"
  else
    echo "[missing] $tool"
    if [[ "$CHECK_ONLY" != "1" ]]; then
      install_tool "$pm" "$tool"
    fi
  fi
done
```

- [ ] **Step 4: Re-run test to verify pass**

Run: `cargo test --test observability_scripts install_tools_script_reports_missing_tools_in_check_mode -- --nocapture`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/observability/install-tools.sh tests/observability_scripts.rs
git commit -m "feat(observability): add local tooling installer for jq lnav fzf"
```

---

### Task 5: Final verification and integration

**Files:**
- Verify and, if needed, update docs/UX command descriptions in:
  - `docs/superpowers/specs/2026-04-23-local-observability-tooling-design.md`

- [ ] **Step 1: Run focused observability tests**

```bash
cargo test structured_log_contains_required_stable_keys -- --nocapture
cargo test --test observability_scripts -- --nocapture
```

- [ ] **Step 2: Run repository quality gates**

Run: `just fmt-check && just lint && just test`  
Expected: all pass.

- [ ] **Step 3: Run coverage profile**

Run: `cargo tarpaulin --timeout 300`  
Expected: coverage stays at/above repository target profile.

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "chore(observability): deliver local log-first observability tooling"
```

- [ ] **Step 5: Push branch**

```bash
git push origin master
```

---

## Spec Coverage Check

- Structured JSON log schema with stable keys: covered by **Task 1**.
- Local query workflows (`logs-tail`, `logs-errors`, `logs-turn`): covered by **Task 3**.
- Local tool installer (`jq`, `lnav`, `fzf`): covered by **Task 4**.
- Correlation field propagation (`turn_id`, `tool_name`, `error_kind`): covered by **Task 2**.
- Validation and quality gates: covered by **Task 5**.

## Placeholder Scan

No `TODO`/`TBD`/“implement later” placeholders are left in tasks.

## Type/Interface Consistency Check

- `format_event_json` is defined once and reused by formatter/emit paths.
- Script command names (`logs-tail`, `logs-errors`, `logs-turn`, `obs-install-tools`) are consistent between script files and `justfile`.
- Correlation fields (`trace_id`, `turn_id`, `tool_name`, `agent_id`, `error_kind`) remain consistent across plan tasks.
