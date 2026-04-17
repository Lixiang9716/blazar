# Resource Directory Design

**Goal:** Add a dedicated resource directory to the repository so future non-code assets have a single, predictable home.

## Scope

- Add a top-level `assets/` directory.
- Keep the initial structure minimal by using a placeholder file so Git tracks the directory.
- Do not move current source files, config files, or scripts into `assets/` as part of this change.
- Do not add runtime asset-loading logic in this change.

## Design

### Directory layout

- `assets/`: top-level resource directory for future static files
- `assets/.gitkeep`: placeholder file to keep the directory in version control

### Usage policy

- New non-code resources should default to `assets/`.
- If the project later accumulates several resource types, it can split into focused subdirectories such as `assets/images/`, `assets/data/`, or `assets/fonts/`.
- The current change intentionally avoids creating subdirectories before they are needed.

### Non-goals

- No migration of existing config into `assets/`
- No movement of ASCII-art data out of Rust source files
- No changes to build, runtime, or test behavior

### Validation

- Confirm `assets/` exists in the repository.
- Confirm the placeholder file keeps the directory tracked.
- Confirm the existing Cargo build and test workflow remains unaffected.
