# Chat View Render Trait Inversion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor chat rendering so `view` orchestration depends on trait contracts and registered render units instead of concrete module calls, while keeping behavior stable.

**Architecture:** Introduce a dedicated render contract layer (`RenderUnit`, `RenderCtx`, `RenderSlot`, `RenderRegistry`) and move orchestration dispatch to registry-driven flow. Migrate timeline/users/input/status/picker rendering behind trait implementations in component-owned modules. Keep `ChatApp` as the sole product-state owner and treat renderers as pure context consumers.

**Tech Stack:** Rust, ratatui, existing Blazar chat view modules, existing test + snapshot infrastructure (`cargo test`, `just fmt-check`, `just lint`, `just test`)

---

## File Structure and Responsibilities

- Create: `src/chat/view/render/mod.rs`
  - Public surface of render-contract layer (`contracts`, `registry`, `units`).
- Create: `src/chat/view/render/contracts.rs`
  - `RenderSlot`, `RenderCtx`, `RenderError`, `RenderUnit`, `RenderRegistry` traits/types.
- Create: `src/chat/view/render/registry.rs`
  - Default registry implementation + slot-to-renderer lookup.
- Create: `src/chat/view/render/units/mod.rs`
  - Unit wiring exports for timeline/users/picker and users sub-panels.
- Create: `src/chat/view/render/units/timeline.rs`
  - `TimelineRenderUnit` adapter around timeline rendering entry.
- Create: `src/chat/view/render/units/users.rs`
  - `UsersTopRenderUnit`, `UsersInputRenderUnit`, `UsersModelRenderUnit`, separators.
- Create: `src/chat/view/render/units/picker.rs`
  - `PickerRenderUnit` adapter.
- Modify: `src/chat/view/mod.rs`
  - Convert orchestration to contract + registry dispatch.
- Modify: `src/chat/view/timeline.rs`
  - Expose component-owned render entry for trait unit invocation.
- Modify: `src/chat/view/users.rs`
  - Expose panel layout helpers and remove direct concrete dispatch assumptions.
- Modify: `src/chat/view/input.rs`
  - Keep input rendering logic component-owned, expose callable boundary for unit.
- Modify: `src/chat/view/status.rs`
  - Keep model/status rendering logic component-owned, expose callable boundary for unit.
- Modify: `src/chat/view/picker.rs`
  - Keep picker rendering component-owned, expose callable boundary for unit.
- Modify: `tests/chat_render.rs`
  - Preserve behavior via regression assertions after dispatch inversion.
- Modify: `tests/chat_render_snapshot.rs`
  - Keep snapshot harness intact after orchestration change.
- Modify: `tests/snapshots/chat_render_snapshot__default_chat_frame.snap` (only if actual rendering output changed).
- Modify: `tests/unit/chat/view/timeline/tests.rs`
  - Keep timeline behavior checks passing through new unit path.
- Create: `tests/unit/chat/view/render_registry/tests.rs`
  - Contract/registry coverage: slot coverage, missing-slot error, fallback behavior.

---

### Task 1: Introduce Render Contracts (TDD)

**Files:**
- Create: `src/chat/view/render/mod.rs`
- Create: `src/chat/view/render/contracts.rs`
- Test: `tests/unit/chat/view/render_registry/tests.rs`

- [ ] **Step 1: Write the failing contract test**

```rust
#[test]
fn render_slot_enum_covers_all_chat_surfaces() {
    use blazar::chat::view::render::contracts::RenderSlot;

    let slots = [
        RenderSlot::Timeline,
        RenderSlot::UsersTop,
        RenderSlot::UsersInput,
        RenderSlot::UsersModel,
        RenderSlot::UsersTopInputSeparator,
        RenderSlot::UsersInputModelSeparator,
        RenderSlot::PickerOverlay,
    ];
    assert_eq!(slots.len(), 7);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test render_slot_enum_covers_all_chat_surfaces`
Expected: FAIL with module/type not found for `chat::view::render::contracts::RenderSlot`.

- [ ] **Step 3: Add minimal contract module**

```rust
// src/chat/view/render/contracts.rs
pub enum RenderSlot {
    Timeline,
    UsersTop,
    UsersInput,
    UsersModel,
    UsersTopInputSeparator,
    UsersInputModelSeparator,
    PickerOverlay,
}
```

- [ ] **Step 4: Add trait and error contract skeleton**

```rust
pub trait RenderUnit {
    fn render(
        &self,
        frame: &mut ratatui_core::terminal::Frame,
        area: ratatui_core::layout::Rect,
        ctx: &RenderCtx<'_>,
    ) -> Result<(), RenderError>;
}

pub enum RenderError {
    RegistryMissingSlot(RenderSlot),
    ComponentError(&'static str),
}
```

- [ ] **Step 5: Run test to verify pass + compile contracts**

Run: `cargo test render_slot_enum_covers_all_chat_surfaces`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/chat/view/render/mod.rs src/chat/view/render/contracts.rs tests/unit/chat/view/render_registry/tests.rs
git commit -m "feat(view): add render contract layer for chat surfaces"
```

---

### Task 2: Build Registry with Explicit Slot Binding (TDD)

**Files:**
- Create: `src/chat/view/render/registry.rs`
- Modify: `src/chat/view/render/contracts.rs`
- Test: `tests/unit/chat/view/render_registry/tests.rs`

- [ ] **Step 1: Write failing registry coverage test**

```rust
#[test]
fn default_registry_resolves_every_required_slot() {
    let registry = DefaultRenderRegistry::for_tests();
    for slot in RenderSlot::all_required() {
        assert!(registry.resolve(slot).is_some(), "missing slot: {slot:?}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test default_registry_resolves_every_required_slot`
Expected: FAIL because `DefaultRenderRegistry` and `RenderSlot::all_required` do not exist.

- [ ] **Step 3: Implement required-slot enumeration**

```rust
impl RenderSlot {
    pub const fn all_required() -> [RenderSlot; 7] {
        [
            RenderSlot::Timeline,
            RenderSlot::UsersTop,
            RenderSlot::UsersInput,
            RenderSlot::UsersModel,
            RenderSlot::UsersTopInputSeparator,
            RenderSlot::UsersInputModelSeparator,
            RenderSlot::PickerOverlay,
        ]
    }
}
```

- [ ] **Step 4: Implement default registry with explicit mapping**

```rust
pub struct DefaultRenderRegistry {
    timeline: TimelineRenderUnit,
    users_top: UsersTopRenderUnit,
    users_input: UsersInputRenderUnit,
    users_model: UsersModelRenderUnit,
    users_sep_top_input: UsersTopInputSeparatorRenderUnit,
    users_sep_input_model: UsersInputModelSeparatorRenderUnit,
    picker: PickerRenderUnit,
}

impl RenderRegistry for DefaultRenderRegistry {
    fn render_slot(&self, slot: RenderSlot, frame: &mut Frame, area: Rect, ctx: &RenderCtx<'_>) -> Result<(), RenderError> {
        match slot {
            RenderSlot::Timeline => self.timeline.render(frame, area, ctx),
            RenderSlot::UsersTop => self.users_top.render(frame, area, ctx),
            RenderSlot::UsersInput => self.users_input.render(frame, area, ctx),
            RenderSlot::UsersModel => self.users_model.render(frame, area, ctx),
            RenderSlot::UsersTopInputSeparator => self.users_sep_top_input.render(frame, area, ctx),
            RenderSlot::UsersInputModelSeparator => self.users_sep_input_model.render(frame, area, ctx),
            RenderSlot::PickerOverlay => self.picker.render(frame, area, ctx),
        }
    }
}
```

- [ ] **Step 5: Add missing-slot error test**

```rust
#[test]
fn registry_returns_explicit_error_for_missing_slot() {
    let registry = EmptyRegistry;
    let err = registry.render_slot(RenderSlot::Timeline, &mut frame, area, &ctx).unwrap_err();
    assert!(matches!(err, RenderError::RegistryMissingSlot(RenderSlot::Timeline)));
}
```

- [ ] **Step 6: Run focused tests**

Run: `cargo test render_registry`
Expected: PASS for slot coverage + explicit missing-slot behavior.

- [ ] **Step 7: Commit**

```bash
git add src/chat/view/render/contracts.rs src/chat/view/render/registry.rs tests/unit/chat/view/render_registry/tests.rs
git commit -m "feat(view): add explicit slot registry for chat renderer dispatch"
```

---

### Task 3: Migrate Orchestrator (`view/mod.rs`) to Registry Dispatch (TDD)

**Files:**
- Modify: `src/chat/view/mod.rs`
- Modify: `src/chat/view/users.rs`
- Modify: `tests/chat_render.rs`
- Test: `tests/chat_render_snapshot.rs`

- [ ] **Step 1: Add failing orchestration test**

```rust
#[test]
fn render_frame_dispatches_slots_without_direct_module_calls() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).unwrap();
    let lines = render_to_lines_for_test(&mut app, 100, 24);
    assert!(lines.iter().any(|line| line.contains("Blazar")));
    assert!(lines.iter().any(|line| line.contains("AUTO")));
}
```

- [ ] **Step 2: Run test to verify baseline failure after temporary direct-call removal**

Run: `cargo test render_frame_dispatches_slots_without_direct_module_calls`
Expected: FAIL while orchestrator has not yet been switched to slot dispatch.

- [ ] **Step 3: Introduce slot layout plan in `view/mod.rs`**

```rust
let slots = users::plan_users_slots(users_area, users_policy, app);
registry.render_slot(RenderSlot::Timeline, frame, timeline_area, &ctx)?;
for slot in slots {
    registry.render_slot(slot.kind, frame, slot.area, &ctx)?;
}
if app.picker.is_visible() {
    registry.render_slot(RenderSlot::PickerOverlay, frame, area, &ctx)?;
}
```

- [ ] **Step 4: Add centralized render error fallback**

```rust
if let Err(err) = registry.render_slot(slot, frame, area, &ctx) {
    render_slot_error_fallback(frame, area, &theme, &err);
}
```

- [ ] **Step 5: Run chat render tests**

Run: `cargo test --test chat_render --test chat_render_snapshot`
Expected: PASS (or single snapshot update if output changed intentionally).

- [ ] **Step 6: Commit**

```bash
git add src/chat/view/mod.rs src/chat/view/users.rs tests/chat_render.rs tests/chat_render_snapshot.rs tests/snapshots/chat_render_snapshot__default_chat_frame.snap
git commit -m "refactor(view): switch frame orchestration to slot registry dispatch"
```

---

### Task 4: Migrate Timeline and Users Components to Render Units (TDD)

**Files:**
- Create: `src/chat/view/render/units/mod.rs`
- Create: `src/chat/view/render/units/timeline.rs`
- Create: `src/chat/view/render/units/users.rs`
- Modify: `src/chat/view/timeline.rs`
- Modify: `src/chat/view/users.rs`
- Modify: `src/chat/view/users/panels.rs`
- Modify: `tests/unit/chat/view/timeline/tests.rs`
- Modify: `tests/chat_render.rs`

- [ ] **Step 1: Add failing unit test for timeline render unit**

```rust
#[test]
fn timeline_render_unit_preserves_banner_and_thinking_behavior() {
    let mut app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).unwrap();
    app.send_message("hello");
    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta { text: "reasoning".into() });
    let lines = render_to_lines_for_test(&mut app, 100, 28);
    let text = lines.join("\n");
    assert!(!text.contains("Describe a task to get started."));
    assert!(text.contains("reasoning"));
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test timeline_render_unit_preserves_banner_and_thinking_behavior`
Expected: FAIL because `TimelineRenderUnit` not implemented.

- [ ] **Step 3: Implement timeline render unit adapter**

```rust
pub struct TimelineRenderUnit;

impl RenderUnit for TimelineRenderUnit {
    fn render(&self, frame: &mut Frame, area: Rect, ctx: &RenderCtx<'_>) -> Result<(), RenderError> {
        crate::chat::view::timeline::render_timeline(frame, area, ctx.app, ctx.theme);
        Ok(())
    }
}
```

- [ ] **Step 4: Implement users slot render units (top/input/model + separators)**

```rust
pub struct UsersInputRenderUnit;
impl RenderUnit for UsersInputRenderUnit {
    fn render(&self, frame: &mut Frame, area: Rect, ctx: &RenderCtx<'_>) -> Result<(), RenderError> {
        crate::chat::view::input::render_input(frame, area, ctx.app, ctx.theme);
        Ok(())
    }
}
```

- [ ] **Step 5: Run timeline + users-focused tests**

Run: `cargo test timeline_ -- --nocapture && cargo test users_area_`
Expected: PASS for prior behavior tests with new dispatch path.

- [ ] **Step 6: Commit**

```bash
git add src/chat/view/render/units/mod.rs src/chat/view/render/units/timeline.rs src/chat/view/render/units/users.rs src/chat/view/timeline.rs src/chat/view/users.rs src/chat/view/users/panels.rs tests/unit/chat/view/timeline/tests.rs tests/chat_render.rs
git commit -m "refactor(view): move timeline and users surfaces behind render units"
```

---

### Task 5: Migrate Input/Status/Picker Units and Finalize Registry Wiring (TDD)

**Files:**
- Create: `src/chat/view/render/units/picker.rs`
- Modify: `src/chat/view/input.rs`
- Modify: `src/chat/view/status.rs`
- Modify: `src/chat/view/picker.rs`
- Modify: `src/chat/view/render/registry.rs`
- Modify: `tests/chat_render.rs`
- Modify: `tests/chat_render_snapshot.rs`

- [ ] **Step 1: Add failing test for picker slot dispatch**

```rust
#[test]
fn picker_overlay_renders_via_registry_slot() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).unwrap();
    app.picker.open();
    let lines = render_to_lines_for_test(&mut app, 100, 35);
    assert!(lines.iter().any(|line| line.contains("navigate")));
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test picker_overlay_renders_via_registry_slot`
Expected: FAIL until picker render unit is bound.

- [ ] **Step 3: Implement picker unit and bind in default registry**

```rust
pub struct PickerRenderUnit;
impl RenderUnit for PickerRenderUnit {
    fn render(&self, frame: &mut Frame, area: Rect, ctx: &RenderCtx<'_>) -> Result<(), RenderError> {
        // uses mutable app path through context adapter
        crate::chat::view::picker::render_picker(frame, area, ctx.app_mut(), ctx.theme);
        Ok(())
    }
}
```

- [ ] **Step 4: Ensure input/status remain component-owned but callable via units**

```rust
pub(crate) fn render_mode_config_row(
    frame: &mut Frame,
    area: Rect,
    app: &ChatApp,
    theme: &ChatTheme,
) {
    // existing status rendering implementation remains in status.rs
}

pub(super) fn render_input(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    // existing input rendering implementation remains in input.rs
}
```

- [ ] **Step 5: Run render regression tests**

Run: `cargo test --test chat_render --test chat_render_snapshot`
Expected: PASS (or approved snapshot update only).

- [ ] **Step 6: Commit**

```bash
git add src/chat/view/render/units/picker.rs src/chat/view/input.rs src/chat/view/status.rs src/chat/view/picker.rs src/chat/view/render/registry.rs tests/chat_render.rs tests/chat_render_snapshot.rs tests/snapshots/chat_render_snapshot__default_chat_frame.snap
git commit -m "refactor(view): route input status and picker rendering through trait units"
```

---

### Task 6: Full Verification, Cleanup, and Handoff

**Files:**
- Modify: `docs/superpowers/specs/2026-04-27-chat-view-render-trait-inversion-design.md` (only if acceptance wording needs sync)
- Modify: plan file progress checkboxes during execution

- [ ] **Step 1: Run formatting gate**

Run: `just fmt-check`
Expected: PASS.

- [ ] **Step 2: Run lint gate**

Run: `just lint`
Expected: PASS.

- [ ] **Step 3: Run full test gate**

Run: `just test`
Expected: PASS.

- [ ] **Step 4: Confirm acceptance criteria against spec**

```text
- view/mod.rs depends on contracts+registry only
- slot coverage includes timeline/users/input/status/picker
- explicit missing-slot errors
- behavior regression tests still pass
```

- [ ] **Step 5: Final commit (if verification adjustments were needed)**

```bash
git add src/chat/view/mod.rs src/chat/view/render src/chat/view/timeline.rs src/chat/view/users.rs src/chat/view/input.rs src/chat/view/status.rs src/chat/view/picker.rs tests/chat_render.rs tests/chat_render_snapshot.rs tests/unit/chat/view/timeline/tests.rs tests/unit/chat/view/render_registry/tests.rs tests/snapshots/chat_render_snapshot__default_chat_frame.snap
git commit -m "chore(view): finalize trait-based chat render inversion"
```
