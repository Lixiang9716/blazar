# App Run Sprite Startup Design

## Problem

`app::run()` currently enters `SchemaUI` immediately, so the integrated slime sprite welcome flow exists in the codebase but is never shown in the normal application entry path.

## Goal

Show the existing sprite-based welcome experience when the app starts, then continue into the current `SchemaUI` flow unchanged.

## Chosen Approach

Use a startup splash sequence in `app::run()`:

1. Enter the existing welcome flow first.
2. Reuse `welcome::startup::run_session(...)` to render the slime sprite animation.
3. After the welcome sequence finishes, continue into the current schema-driven UI.

This keeps the feature isolated to startup flow wiring and avoids modifying `SchemaUI` layout behavior.

## Alternatives Considered

### 1. Embed the sprite inside SchemaUI

- **Pros:** mascot stays visible throughout the form flow
- **Cons:** requires reshaping third-party UI rendering or surrounding layout; higher risk and more moving parts

### 2. Print a single static sprite before SchemaUI

- **Pros:** minimal implementation
- **Cons:** discards the existing animation and state-driven welcome behavior

## Architecture

### `app::run()`

- Split startup flow so the function performs:
  1. welcome rendering
  2. schema loading
  3. `SchemaUI` execution
- Keep the public `run()` entry point unchanged for `main.rs`.

### Welcome integration

- Reuse `welcome::startup::run_session(...)` rather than duplicating render logic.
- Feed it real `stdin`/`stdout` so the existing terminal experience appears during normal startup.

### SchemaUI handoff

- After the welcome sequence completes, proceed with the existing `SchemaUI::new(schema)...run()?` path unchanged.

## Data Flow

1. `main()` calls `app::run()`.
2. `app::run()` runs the welcome splash on terminal I/O.
3. When the splash completes, `app::run()` loads config and launches `SchemaUI`.
4. When `SchemaUI` completes, the JSON result is printed as it is today.

## Error Handling

- If the welcome startup flow fails, return that error from `app::run()`.
- If config loading or `SchemaUI` fails, preserve current behavior and bubble the error up.
- Do not silently skip the welcome flow on failure.

## Testing

- Add a focused test seam around `app` startup orchestration so the order can be verified without driving real terminal I/O.
- Verify the startup path performs welcome-before-schema sequencing.
- Preserve existing config/schema tests and welcome tests.

## Scope

In scope:

- Showing the existing slime sprite welcome flow before `SchemaUI`
- Minimal refactor needed to make startup order testable

Out of scope:

- Replacing the mascot asset
- Embedding the mascot inside `SchemaUI`
- Reworking the welcome copy, timing, or animation behavior
