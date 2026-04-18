# Config Directory Design

**Goal:** Move Blazar's runtime UI schema and default application values into a dedicated `config/` directory while keeping Rust and Cargo files in their required root locations.

## Scope

- Keep `Cargo.toml`, `Cargo.lock`, and other toolchain-required root files where they are.
- Create `config/app.json` as the single source of truth for the SchemaUI JSON schema currently embedded in `src/app.rs`.
- Add Rust-side loading code so the application reads `config/app.json` at startup instead of constructing the schema inline.

## Design

### File layout

- `config/app.json`: runtime application schema and default values
- `src/config.rs`: config path constant, JSON loading, parse/read error handling
- `src/app.rs`: load schema, launch SchemaUI, print captured value

### Loading flow

1. `run()` asks the config module for the application schema.
2. The config module reads `config/app.json`.
3. The config module parses the file into `serde_json::Value`.
4. `run()` passes the loaded value into `SchemaUI::new(...)`.

### Failure behavior

- Missing config file should return an explicit read error.
- Invalid JSON should return an explicit parse error.
- The application should not silently fall back to hardcoded defaults.

### Testing

- Add tests that lock the config file path to `config/app.json`.
- Add tests that load the bundled config file and assert key defaults are present.
- Add tests that load a temporary JSON config file to verify the file loader works independently of the bundled config.

## Out of scope

- Moving Cargo metadata into `config/`
- Adding environment-specific config overlays
- Redesigning the SchemaUI structure itself
