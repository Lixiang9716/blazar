# App Run Sprite Startup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show the existing slime sprite welcome sequence during `app::run()` startup, then continue into the current `SchemaUI` flow unchanged.

**Architecture:** Refactor `src/app.rs` so startup orchestration is isolated in a small helper that can be tested with injected closures. Keep the real entry path thin: run welcome on real terminal I/O, then build the schema, run `SchemaUI`, and print the JSON result exactly as before.

**Tech Stack:** Rust, `schemaui`, `serde_json`, existing `welcome::startup` module

---

## File Structure

- Modify: `src/app.rs`
  - Add a small orchestration helper for welcome-before-schema sequencing
  - Add real startup adapters for welcome session and `SchemaUI`
  - Extend unit tests for ordering and error propagation
- Verify existing behavior with:
  - `tests/welcome_startup.rs`
  - `tests/app_config.rs`

### Task 1: Add a testable startup orchestration seam

**Files:**
- Modify: `src/app.rs`
- Verify: `tests/welcome_startup.rs`
- Verify: `tests/app_config.rs`

- [ ] **Step 1: Write the failing tests in `src/app.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::{build_schema, run_flow};
    use serde_json::json;
    use std::cell::RefCell;
    use std::io;

    #[test]
    fn run_flow_runs_welcome_before_schema_ui() {
        let calls = RefCell::new(Vec::new());

        let value = run_flow(
            || {
                calls.borrow_mut().push("welcome");
                Ok(())
            },
            || {
                calls.borrow_mut().push("schema");
                Ok(json!({
                    "title": "Blazar",
                    "type": "object",
                    "properties": {}
                }))
            },
            |schema| {
                assert_eq!(schema["title"], "Blazar");
                calls.borrow_mut().push("ui");
                Ok(json!({"request": "ok"}))
            },
        )
        .expect("startup flow should succeed");

        assert_eq!(value["request"], "ok");
        assert_eq!(*calls.borrow(), vec!["welcome", "schema", "ui"]);
    }

    #[test]
    fn run_flow_bubbles_welcome_errors_without_loading_schema() {
        let calls = RefCell::new(Vec::new());

        let error = run_flow(
            || {
                calls.borrow_mut().push("welcome");
                Err(io::Error::other("welcome failed"))
            },
            || {
                calls.borrow_mut().push("schema");
                build_schema()
            },
            |_schema| unreachable!("schema ui should not run after welcome failure"),
        )
        .expect_err("welcome failure should bubble up");

        assert!(error.to_string().contains("welcome failed"));
        assert_eq!(*calls.borrow(), vec!["welcome"]);
    }
}
```

- [ ] **Step 2: Run the focused tests to verify they fail**

Run:

```bash
cargo test --quiet run_flow_runs_welcome_before_schema_ui
cargo test --quiet run_flow_bubbles_welcome_errors_without_loading_schema
```

Expected:

```text
error[E0432]: unresolved import `super::run_flow`
```

- [ ] **Step 3: Add the minimal orchestration helper**

```rust
use std::io;

type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

fn run_flow<W, B, S>(run_welcome: W, build_schema: B, run_schema: S) -> AppResult<Value>
where
    W: FnOnce() -> io::Result<()>,
    B: FnOnce() -> Result<Value, config::ConfigError>,
    S: FnOnce(Value) -> AppResult<Value>,
{
    run_welcome()?;
    let schema = build_schema()?;
    run_schema(schema)
}
```

- [ ] **Step 4: Run the focused tests to verify they pass**

Run:

```bash
cargo test --quiet run_flow_runs_welcome_before_schema_ui
cargo test --quiet run_flow_bubbles_welcome_errors_without_loading_schema
```

Expected:

```text
running 1 test
.
test result: ok. 1 passed
```

- [ ] **Step 5: Commit the seam and tests**

```bash
git add src/app.rs
git commit -m "test(app): add startup flow seam"
```

### Task 2: Wire the real welcome splash into `app::run()`

**Files:**
- Modify: `src/app.rs`
- Verify: `tests/welcome_startup.rs`
- Verify: `tests/app_config.rs`

- [ ] **Step 1: Write the next failing test for real startup wiring**

```rust
#[test]
fn run_app_prints_serialized_value_after_startup_flow() {
    let calls = RefCell::new(Vec::new());
    let printed = RefCell::new(String::new());

    run_app(
        || {
            calls.borrow_mut().push("welcome");
            Ok(())
        },
        || Ok(json!({"title": "Blazar", "type": "object", "properties": {}})),
        |_schema| {
            calls.borrow_mut().push("ui");
            Ok(json!({"delivery": {"format": "text"}}))
        },
        |json| {
            calls.borrow_mut().push("print");
            printed.borrow_mut().push_str(&json);
            Ok(())
        },
    )
    .expect("startup flow should succeed");

    assert!(printed.borrow().contains("\"delivery\""));
    assert_eq!(*calls.borrow(), vec!["welcome", "ui", "print"]);
}
```

- [ ] **Step 2: Run the focused test to verify it fails for the current wiring**

Run:

```bash
cargo test --quiet run_app_prints_serialized_value_after_startup_flow
```

Expected:

```text
error[E0425]: cannot find function `run_app` in this scope
```

- [ ] **Step 3: Implement the real startup adapters and update `run()`**

```rust
use std::io;

fn run_welcome_session() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();

    crate::welcome::startup::run_session(&mut input, &mut output)
}

fn run_schema_ui(schema: Value) -> AppResult<Value> {
    let title = config::schema_title(&schema)?.to_owned();
    let value = SchemaUI::new(schema)
        .with_title(&title)
        .with_options(UiOptions::default())
        .run()?;

    Ok(value)
}

fn run_app<W, B, S, P>(
    run_welcome: W,
    build_schema: B,
    run_schema: S,
    print_json: P,
) -> AppResult<()>
where
    W: FnOnce() -> io::Result<()>,
    B: FnOnce() -> Result<Value, config::ConfigError>,
    S: FnOnce(Value) -> AppResult<Value>,
    P: FnOnce(String) -> AppResult<()>,
{
    let value = run_flow(run_welcome, build_schema, run_schema)?;
    let json = serde_json::to_string_pretty(&value)?;
    print_json(json)
}

pub fn run() -> AppResult<()> {
    run_app(run_welcome_session, build_schema, run_schema_ui, |json| {
        println!("{json}");
        Ok(())
    })
}
```

- [ ] **Step 4: Run targeted and broad verification**

Run:

```bash
cargo test --quiet run_app_prints_serialized_value_after_startup_flow
cargo test --quiet welcome_startup
cargo test --quiet app_config
cargo test --quiet
```

Expected:

```text
test result: ok.
```

- [ ] **Step 5: Commit the startup integration**

```bash
git add src/app.rs
git commit -m "feat(app): show sprite welcome before schema ui"
```
