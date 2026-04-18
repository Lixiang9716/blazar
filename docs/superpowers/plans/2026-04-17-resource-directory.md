# Resource Directory Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a tracked top-level `assets/` directory so the repository has a stable home for future non-code resources.

**Architecture:** The change stays intentionally small: create `assets/`, keep it in version control with a placeholder file, and add one repository-level test that proves the directory contract exists. No runtime loading, file migration, or subdirectory hierarchy is introduced in this change.

**Tech Stack:** Rust 2024, Cargo tests, Git

---

### Task 1: Create the tracked assets directory

**Files:**
- Create: `assets/.gitkeep`
- Create: `tests/assets_layout.rs`

- [ ] **Step 1: Write the failing test**

```rust
use std::path::Path;

#[test]
fn repository_keeps_a_tracked_assets_directory() {
    assert!(
        Path::new("assets/.gitkeep").is_file(),
        "assets/.gitkeep should exist so the assets directory stays tracked"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --quiet assets_layout`
Expected: FAIL because `tests/assets_layout.rs` exists but `assets/.gitkeep` has not been created yet.

- [ ] **Step 3: Write minimal implementation**

Create `assets/.gitkeep` as an empty file.

Create `tests/assets_layout.rs` with:

```rust
use std::path::Path;

#[test]
fn repository_keeps_a_tracked_assets_directory() {
    assert!(
        Path::new("assets/.gitkeep").is_file(),
        "assets/.gitkeep should exist so the assets directory stays tracked"
    );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --quiet assets_layout`
Expected: PASS with the new repository layout test green.

- [ ] **Step 5: Run the full suite**

Run: `cargo test --quiet`
Expected: PASS with existing tests unaffected.

- [ ] **Step 6: Commit**

```bash
git add assets/.gitkeep tests/assets_layout.rs
git commit -m "test: track assets directory layout

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```
