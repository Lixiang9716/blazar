# Config Directory Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the runtime SchemaUI configuration into `config/app.json` and load it from Rust instead of hardcoding it in `src/app.rs`.

**Architecture:** The application schema will live as JSON in `config/app.json`. A new `src/config.rs` module will own the config path and JSON loading behavior, and `src/app.rs` will consume the loaded schema to run the UI. Toolchain-required root files stay in place.

**Tech Stack:** Rust 2024, serde_json, Cargo tests

---

### Task 1: Add failing config tests

**Files:**
- Create: `tests/app_config.rs`
- Test: `tests/app_config.rs`

- [ ] **Step 1: Write the failing test**

```rust
use blazar::config::{APP_SCHEMA_PATH, load_app_schema, load_app_schema_from_path};
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --quiet`
Expected: FAIL because `blazar::config` does not exist yet.

- [ ] **Step 3: Commit**

```bash
git add tests/app_config.rs
git commit -m "test: add config directory coverage"
```

### Task 2: Add config loading module and bundled JSON schema

**Files:**
- Create: `src/config.rs`
- Create: `config/app.json`
- Modify: `src/lib.rs`
- Test: `tests/app_config.rs`

- [ ] **Step 1: Write minimal implementation**

```rust
pub const APP_SCHEMA_PATH: &str = "config/app.json";
```

- [ ] **Step 2: Load JSON from disk**

```rust
pub fn load_app_schema_from_path(path: impl AsRef<std::path::Path>) -> Result<serde_json::Value, ConfigError> {
    // read file and deserialize JSON
}
```

- [ ] **Step 3: Add bundled runtime schema**

```json
{
  "title": "Blazar Mission Console"
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --quiet`
Expected: PASS for the new config tests.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/lib.rs config/app.json tests/app_config.rs
git commit -m "feat: load app schema from config directory"
```

### Task 3: Wire the app entrypoint to the config module

**Files:**
- Modify: `src/app.rs`
- Test: `src/app.rs`

- [ ] **Step 1: Replace inline schema construction with config loading**

```rust
let value = SchemaUI::new(build_schema()?)
```

- [ ] **Step 2: Keep schema-focused assertions green**

```rust
let schema = build_schema().expect("schema should load from config/app.json");
```

- [ ] **Step 3: Run tests to verify the full suite passes**

Run: `cargo test --quiet`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/app.rs
git commit -m "refactor: read app schema from config file"
```
