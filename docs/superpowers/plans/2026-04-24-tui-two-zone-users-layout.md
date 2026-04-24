# Two-Zone TUI Users Region Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rework chat TUI into `timeline + users` two zones, with inline slash command status, multiline `>` input, `AUTO/PLAN` mode toggle (`Shift+Tab`), and mode/context/git/reference metadata in users region.

**Architecture:** Keep all UI/product state inside `ChatApp`; render layer becomes a pure projection of timeline and users-region state. Replace slash-triggered modal behavior with status-row command hints while preserving existing command registry execution path. Convert banner/thinking into timeline-native entries so the top region is one coherent entry stream.

**Tech Stack:** Rust, ratatui, crossterm, ratatui_textarea, existing `chat` app/view/runtime modules, cargo test, just

---

## File Structure and Responsibilities

- Create: `src/chat/users_state.rs`
  - Shared state types for users region (`UserMode`, `StatusMode`, `ContextUsage`, `UsersStatusSnapshot`).
- Modify: `src/chat/app.rs`
  - Own new users-region state and expose read-only snapshot accessors.
- Modify: `src/chat/input.rs`
  - Add keyboard actions for `Shift+Enter` (newline) and `Shift+Tab` (mode toggle).
- Modify: `src/chat/app/actions.rs`
  - Wire new actions and slash inline behavior.
- Modify: `src/chat/app/turns.rs`
  - Route submit by `UserMode` (`Auto` vs `Plan`).
- Modify: `src/chat/model.rs`
  - Add timeline banner entry kind.
- Modify: `src/chat/app/events.rs`
  - Keep thinking entries visible and update reference-file/context state from events.
- Modify: `src/chat/view/mod.rs`
  - Two-zone frame layout (`timeline_area`, `users_area` only).
- Modify: `src/chat/view/status.rs`
  - Render users-region status row + mode row.
- Modify: `src/chat/view/input.rs`
  - Render multiline `> ` composer with dynamic row height.
- Modify: `src/chat/view/timeline.rs`
  - Render `Banner` + `Thinking` entries.
- Modify: `tests/unit/chat/app/tests_impl.inc`
  - State and action behavior tests.
- Modify: `tests/chat_render.rs`
  - Snapshot/behavior coverage for two-zone + users rows + slash inline list.
- Modify: `tests/unit/chat/view/timeline/tests.rs`
  - Timeline rendering tests for banner/thinking entries.

### Task 1: Add users-region state model in ChatApp

**Files:**
- Create: `src/chat/users_state.rs`
- Modify: `src/chat/app.rs`
- Test: `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Write failing test for default users-region state**

```rust
#[test]
fn users_region_defaults_to_auto_and_normal_status() {
    let app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("app should initialize");
    let snapshot = app.users_status_snapshot();

    assert_eq!(snapshot.mode, UserMode::Auto);
    assert_eq!(snapshot.status_mode, StatusMode::Normal);
    assert!(snapshot.context_usage.is_none());
}
```

- [ ] **Step 2: Run the failing test**

Run: `cargo test --lib chat::app::tests::users_region_defaults_to_auto_and_normal_status -- --exact`  
Expected: FAIL with missing `users_status_snapshot`/types.

- [ ] **Step 3: Implement users state types and app fields**

```rust
// src/chat/users_state.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserMode {
    Auto,
    Plan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusMode {
    Normal,
    CommandList,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextUsage {
    pub used_tokens: u32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsersStatusSnapshot {
    pub mode: UserMode,
    pub status_mode: StatusMode,
    pub current_path: String,
    pub branch: String,
    pub pr_label: Option<String>,
    pub referenced_files: Vec<String>,
    pub model_name: String,
    pub context_usage: Option<ContextUsage>,
}
```

```rust
// src/chat/app.rs (fields + accessor)
use crate::chat::users_state::{ContextUsage, StatusMode, UserMode, UsersStatusSnapshot};

pub struct ChatApp {
    // ...
    user_mode: UserMode,
    users_status_mode: StatusMode,
    git_pr_label: Option<String>,
    referenced_files: Vec<String>,
    context_usage: Option<ContextUsage>,
}

pub fn users_status_snapshot(&self) -> UsersStatusSnapshot {
    UsersStatusSnapshot {
        mode: self.user_mode,
        status_mode: self.users_status_mode,
        current_path: self.display_path.clone(),
        branch: self.branch.clone(),
        pr_label: self.git_pr_label.clone(),
        referenced_files: self.referenced_files.clone(),
        model_name: self.model_name.clone(),
        context_usage: self.context_usage.clone(),
    }
}
```

- [ ] **Step 4: Re-run the test**

Run: `cargo test --lib chat::app::tests::users_region_defaults_to_auto_and_normal_status -- --exact`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/users_state.rs src/chat/app.rs tests/unit/chat/app/tests_impl.inc
git commit -m "feat(chat): add users-region state model"
```

### Task 2: Implement two-zone frame layout and users subrows

**Files:**
- Modify: `src/chat/view/mod.rs`
- Modify: `src/chat/view/status.rs`
- Modify: `src/chat/view/input.rs`
- Test: `tests/chat_render.rs`

- [ ] **Step 1: Write failing render test for two-zone users rows**

```rust
#[test]
fn chat_view_renders_status_input_mode_rows_in_users_region() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app should initialize");
    let lines = render_to_lines_for_test(&mut app, 100, 22);

    assert!(lines.iter().any(|line| line.contains("> ")));
    assert!(lines.iter().any(|line| line.contains("AUTO")));
    assert!(lines.iter().any(|line| line.contains("echo")));
}
```

- [ ] **Step 2: Run failing render test**

Run: `cargo test chat_view_renders_status_input_mode_rows_in_users_region -- --exact`  
Expected: FAIL before layout refactor.

- [ ] **Step 3: Refactor `render_frame` into timeline + users**

```rust
// src/chat/view/mod.rs
let [timeline_area, users_area] = vertical![>=1, ==users_height(app, area.height)].areas(area);
let [status_area, input_area, mode_area] = vertical![==1, >=1, ==1].areas(users_area);

timeline::render_timeline(frame, timeline_area, app, &theme);
status::render_users_status_row(frame, status_area, app, &theme);
input::render_input(frame, input_area, app, &theme);
status::render_mode_config_row(frame, mode_area, app, &theme);
```

```rust
// src/chat/view/status.rs
pub(super) fn render_mode_config_row(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let snap = app.users_status_snapshot();
    let mode = match snap.mode { UserMode::Auto => "AUTO", UserMode::Plan => "PLAN" };
    let ctx = snap.context_usage
        .as_ref()
        .map(|u| format!("{}/{} ({}%)", u.used_tokens, u.max_tokens, (u.used_tokens * 100 / u.max_tokens.max(1))))
        .unwrap_or_else(|| "n/a".to_string());
    // render: [MODE] ... model + context
}
```

- [ ] **Step 4: Re-run render test**

Run: `cargo test chat_view_renders_status_input_mode_rows_in_users_region -- --exact`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/view/mod.rs src/chat/view/status.rs src/chat/view/input.rs tests/chat_render.rs
git commit -m "refactor(chat): render two-zone layout with users subrows"
```

### Task 3: Make banner and thinking timeline-native entries

**Files:**
- Modify: `src/chat/model.rs`
- Modify: `src/chat/app.rs`
- Modify: `src/chat/view/timeline.rs`
- Test: `tests/unit/chat/view/timeline/tests.rs`

- [ ] **Step 1: Write failing timeline test for banner/thinking visibility**

```rust
#[test]
fn timeline_renders_banner_and_thinking_entries() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app should initialize");
    app.apply_agent_event_for_test(AgentEvent::ThinkingDelta { text: "reasoning".into() });
    let lines = render_to_lines_for_test(&mut app, 100, 28);

    assert!(lines.iter().any(|line| line.contains("Welcome")));
    assert!(lines.iter().any(|line| line.contains("reasoning")));
}
```

- [ ] **Step 2: Run failing test**

Run: `cargo test --lib chat::view::timeline::tests::timeline_renders_banner_and_thinking_entries -- --exact`  
Expected: FAIL because banner is not timeline entry and thinking is filtered out.

- [ ] **Step 3: Implement `EntryKind::Banner` and render thinking**

```rust
// src/chat/model.rs
pub enum EntryKind {
    Message,
    Warning,
    Hint,
    Banner,
    // ...
    Thinking,
}
```

```rust
// src/chat/app.rs (initial timeline seed)
timeline: vec![
    TimelineEntry {
        actor: Actor::System,
        kind: EntryKind::Banner,
        title: Some("Blazar".to_owned()),
        body: "Describe a task to get started.".to_owned(),
        details: String::new(),
    },
    TimelineEntry::response("Tell me what you'd like to explore."),
],
```

```rust
// src/chat/view/timeline.rs
for entry in app.timeline() {
    // remove global `if entry.kind == EntryKind::Thinking { continue; }`
    let entry_lines = render_entry(entry, theme, content_width);
    lines.extend(entry_lines);
}
```

- [ ] **Step 4: Re-run timeline test**

Run: `cargo test --lib chat::view::timeline::tests::timeline_renders_banner_and_thinking_entries -- --exact`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/model.rs src/chat/app.rs src/chat/view/timeline.rs tests/unit/chat/view/timeline/tests.rs
git commit -m "feat(chat): render banner and thinking as timeline entries"
```

### Task 4: Replace slash overlay with inline status command list

**Files:**
- Modify: `src/chat/app/actions.rs`
- Modify: `src/chat/view/status.rs`
- Modify: `src/chat/app.rs`
- Test: `tests/unit/chat/app/tests_impl.inc`
- Test: `tests/chat_render.rs`

- [ ] **Step 1: Write failing test for slash inline mode (no modal picker)**

```rust
#[test]
fn slash_input_switches_to_inline_command_status_mode() {
    let mut app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE)));

    assert!(!app.picker.is_open());
    assert_eq!(app.users_status_snapshot().status_mode, StatusMode::CommandList);
}
```

- [ ] **Step 2: Run failing test**

Run: `cargo test --lib chat::app::tests::slash_input_switches_to_inline_command_status_mode -- --exact`  
Expected: FAIL because `/` currently opens picker.

- [ ] **Step 3: Implement slash status-mode path**

```rust
// src/chat/app/actions.rs
InputAction::Key(key) => {
    self.composer.input(key);
    let text = self.composer_text();
    if text.starts_with('/') {
        self.users_status_mode = StatusMode::CommandList;
        self.refresh_inline_command_matches(&text);
    } else {
        self.users_status_mode = StatusMode::Normal;
        self.inline_command_matches.clear();
    }
}
```

```rust
// src/chat/view/status.rs
pub(super) fn render_users_status_row(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let snap = app.users_status_snapshot();
    let line = match snap.status_mode {
        StatusMode::Normal => format_status_line(&snap),
        StatusMode::CommandList => format_command_matches_line(app.inline_command_matches_for_render()),
    };
    frame.render_widget(Paragraph::new(line).style(theme.status_bar), area);
}
```

- [ ] **Step 4: Re-run slash tests**

Run:  
`cargo test --lib chat::app::tests::slash_input_switches_to_inline_command_status_mode -- --exact && cargo test chat_view_renders_inline_command_matches_in_status_row -- --exact`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/app/actions.rs src/chat/app.rs src/chat/view/status.rs tests/unit/chat/app/tests_impl.inc tests/chat_render.rs
git commit -m "feat(chat): show slash command list inline in users status row"
```

### Task 5: Add multiline `> ` input and mode toggle actions

**Files:**
- Modify: `src/chat/input.rs`
- Modify: `src/chat/app/actions.rs`
- Modify: `src/chat/view/input.rs`
- Test: `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Write failing tests for newline insert + mode toggle**

```rust
#[test]
fn shift_enter_inserts_newline_without_submit() {
    let mut app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("app should initialize");
    app.handle_action(InputAction::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)));
    app.handle_action(InputAction::InsertNewline);
    app.handle_action(InputAction::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE)));
    assert_eq!(app.composer_text(), "a\nb");
}

#[test]
fn shift_tab_toggles_user_mode() {
    let mut app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("app should initialize");
    assert_eq!(app.users_status_snapshot().mode, UserMode::Auto);
    app.handle_action(InputAction::ToggleMode);
    assert_eq!(app.users_status_snapshot().mode, UserMode::Plan);
}
```

- [ ] **Step 2: Run failing tests**

Run:  
`cargo test --lib chat::app::tests::shift_enter_inserts_newline_without_submit -- --exact && cargo test --lib chat::app::tests::shift_tab_toggles_user_mode -- --exact`  
Expected: FAIL due missing actions.

- [ ] **Step 3: Add actions and handler paths**

```rust
// src/chat/input.rs
pub enum InputAction {
    // ...
    InsertNewline,
    ToggleMode,
}

match (key.code, key.modifiers) {
    (KeyCode::Enter, KeyModifiers::SHIFT) => InputAction::InsertNewline,
    (KeyCode::BackTab, _) => InputAction::ToggleMode,
    (KeyCode::Enter, _) => InputAction::Submit,
    // ...
}
```

```rust
// src/chat/app/actions.rs
InputAction::InsertNewline => self.composer.insert_newline(),
InputAction::ToggleMode => {
    self.user_mode = match self.user_mode {
        UserMode::Auto => UserMode::Plan,
        UserMode::Plan => UserMode::Auto,
    };
}
```

```rust
// src/chat/view/input.rs
let prompt = Paragraph::new(Line::from(Span::styled("> ", theme.input_prompt)));
```

- [ ] **Step 4: Re-run action tests**

Run:  
`cargo test --lib chat::app::tests::shift_enter_inserts_newline_without_submit -- --exact && cargo test --lib chat::app::tests::shift_tab_toggles_user_mode -- --exact`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/input.rs src/chat/app/actions.rs src/chat/view/input.rs tests/unit/chat/app/tests_impl.inc
git commit -m "feat(chat): add multiline input and shift-tab mode toggle"
```

### Task 6: Route submit by mode and render model/context usage

**Files:**
- Modify: `src/chat/app/turns.rs`
- Modify: `src/chat/view/status.rs`
- Modify: `src/chat/app/events.rs`
- Test: `tests/unit/chat/app/tests_impl.inc`
- Test: `tests/chat_render.rs`

- [ ] **Step 1: Write failing tests for plan-mode submit routing and context text**

```rust
#[test]
fn submit_in_plan_mode_uses_plan_prompt_path() {
    let mut app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR")).expect("app should initialize");
    app.handle_action(InputAction::ToggleMode); // PLAN
    app.set_composer_text("optimize parser");
    app.submit_composer();
    assert!(app.timeline().iter().any(|e| e.body.contains("/plan") || e.body.contains("Create a concise implementation plan")));
}
```

```rust
#[test]
fn mode_row_renders_context_ratio_when_available() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app should initialize");
    app.set_context_usage_for_test(1200, 8000);
    let lines = render_to_lines_for_test(&mut app, 120, 24);
    assert!(lines.iter().any(|line| line.contains("1200/8000")));
}
```

- [ ] **Step 2: Run failing tests**

Run:  
`cargo test --lib chat::app::tests::submit_in_plan_mode_uses_plan_prompt_path -- --exact && cargo test mode_row_renders_context_ratio_when_available -- --exact`  
Expected: FAIL before mode-aware routing/context rendering.

- [ ] **Step 3: Implement mode-aware submit + context formatting**

```rust
// src/chat/app/turns.rs
let dispatch = match self.user_mode {
    UserMode::Plan => PendingDispatch::Runtime {
        runtime_prompt: build_plan_prompt(trimmed),
        kind: TurnKind::Plan,
    },
    UserMode::Auto => build_pending_dispatch(trimmed),
};
```

```rust
// src/chat/view/status.rs (mode row right side)
let ctx_text = match snap.context_usage {
    Some(ContextUsage { used_tokens, max_tokens }) if max_tokens > 0 => {
        format!("{used_tokens}/{max_tokens} ({}%)", used_tokens * 100 / max_tokens)
    }
    _ => "n/a".to_string(),
};
let right = format!("{} · ctx {}", snap.model_name, ctx_text);
```

- [ ] **Step 4: Re-run mode/context tests**

Run:  
`cargo test --lib chat::app::tests::submit_in_plan_mode_uses_plan_prompt_path -- --exact && cargo test mode_row_renders_context_ratio_when_available -- --exact`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/app/turns.rs src/chat/view/status.rs src/chat/app/events.rs tests/unit/chat/app/tests_impl.inc tests/chat_render.rs
git commit -m "feat(chat): route submit by mode and show model context ratio"
```

### Task 7: Integrate status metadata (path/branch/PR/references) and finalize gates

**Files:**
- Modify: `src/chat/view/status.rs`
- Modify: `src/chat/app.rs`
- Modify: `src/chat/app/events.rs`
- Test: `tests/chat_render.rs`
- Test: `tests/unit/chat/app/tests_impl.inc`

- [ ] **Step 1: Add failing status render tests for branch/PR/references**

```rust
#[test]
fn status_row_renders_path_branch_pr_and_references() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app should initialize");
    app.set_pr_label_for_test(Some("PR#42 improve timeline".to_string()));
    app.set_referenced_files_for_test(vec!["src/chat/view/mod.rs".to_string()]);
    let lines = render_to_lines_for_test(&mut app, 130, 24);

    assert!(lines.iter().any(|line| line.contains("~/blazar")));
    assert!(lines.iter().any(|line| line.contains("main")));
    assert!(lines.iter().any(|line| line.contains("PR#42")));
    assert!(lines.iter().any(|line| line.contains("src/chat/view/mod.rs")));
}
```

- [ ] **Step 2: Run failing status metadata test**

Run: `cargo test status_row_renders_path_branch_pr_and_references -- --exact`  
Expected: FAIL before metadata wiring.

- [ ] **Step 3: Implement metadata formatting and fallback behavior**

```rust
// src/chat/view/status.rs
fn format_status_line(snap: &UsersStatusSnapshot) -> String {
    let mut left = vec![snap.current_path.clone()];
    if !snap.branch.is_empty() {
        left.push(format!("({})", snap.branch));
    }
    if let Some(pr) = &snap.pr_label {
        left.push(format!("[{}]", pr));
    }
    let refs = if snap.referenced_files.is_empty() {
        "refs: -".to_string()
    } else {
        format!("refs: {}", snap.referenced_files.join(", "))
    };
    format!("{} · {}", left.join(" "), refs)
}
```

- [ ] **Step 4: Run repository quality gates**

Run:  
`just fmt-check && just lint && just test`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/chat/view/status.rs src/chat/app.rs src/chat/app/events.rs tests/chat_render.rs tests/unit/chat/app/tests_impl.inc
git commit -m "feat(chat): show path branch pr and references in users status row"
```

## Plan Self-Review

### 1. Spec coverage

- Two-zone layout and three users subrows: Task 2.
- Banner/thinking as timeline entries: Task 3.
- Slash updates status command list (no panel): Task 4.
- `> ` multiline input + wrapping + submit semantics: Task 5.
- `Shift+Tab` auto/plan mode and mode-aware submit: Task 5 + Task 6.
- Model/context ratio display: Task 6.
- Path/branch/PR/references status content: Task 7.

No spec coverage gaps found.

### 2. Placeholder scan

- No TBD/TODO placeholders.
- Each code step includes concrete snippets.
- Each test/run step includes explicit commands and expected outcomes.

### 3. Type consistency

- `UserMode`, `StatusMode`, and `ContextUsage` names are used consistently across tasks.
- `users_status_snapshot()` is the single read API referenced by render and tests.
- `InputAction::InsertNewline` and `InputAction::ToggleMode` are consistently named in input/actions/tests.
