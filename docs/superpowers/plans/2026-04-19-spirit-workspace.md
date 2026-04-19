# Spirit Workspace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the current Spirit chat TUI into a workspace-style interface with first-class Chat, Git, and Sessions views.

**Architecture:** Keep `ChatApp` focused on chat timeline and composer state, then add a higher-level workspace shell that owns active view, git/session summary data, and focus/navigation state. Render the workspace through one shared shell so Chat stays the default home, while Git and Sessions remain lightweight read-oriented panels that share the same header, rail, and footer.

**Tech Stack:** Rust 2024, `ratatui`, `crossterm`, `ratatui-textarea`, `insta`, `proptest`, Git CLI

---

## File structure

### Create

- `src/chat/workspace.rs`
  - Workspace-level state: active view, focus, git/session summary structs, seeded test data
- `src/chat/git.rs`
  - Lightweight Git summary model plus shell-backed loader for branch/status/commit metadata
- `src/chat/session.rs`
  - Session summary model plus loader for plan/checkpoint/todo counts
- `tests/chat_workspace.rs`
  - Workspace-state behavior tests for view switching and focus rules
- `tests/chat_git_view.rs`
  - Git view render tests with fixed sample data
- `tests/chat_session_view.rs`
  - Sessions view render tests with fixed sample data

### Modify

- `src/chat/mod.rs`
  - Export workspace, git, and session modules
- `src/chat/input.rs`
  - Extend input actions for view switching and focus cycling
- `src/chat/view.rs`
  - Replace two-pane chat-only render shell with workspace shell + responsive layouts
- `src/chat/theme.rs`
  - Add workspace shell styles, active-nav styles, header/footer/status styles
- `src/chat/app.rs`
  - Run the terminal loop through the workspace shell instead of raw `ChatApp`
- `tests/chat_render.rs`
  - Adapt shell/layout assertions to the new workspace render
- `tests/chat_render_snapshot.rs`
  - Update snapshot coverage to the new shell
- `tests/snapshots/chat_render_snapshot__default_chat_frame.snap`
  - New default workspace snapshot
- `tests/chat_runtime.rs`
  - Add key-action/runtime coverage for workspace shortcuts
- `tests/chat_input_props.rs`
  - Extend key mapping assertions for the new shortcuts

## Task 1: Add workspace shell state

**Files:**
- Create: `src/chat/workspace.rs`
- Modify: `src/chat/mod.rs`
- Test: `tests/chat_workspace.rs`

- [ ] **Step 1: Write the failing workspace-state test**

```rust
use blazar::chat::workspace::{WorkspaceApp, WorkspaceFocus, WorkspaceView};

#[test]
fn workspace_switches_views_and_cycles_focus() {
    let mut app = WorkspaceApp::new_for_test(env!("CARGO_MANIFEST_DIR"));

    assert_eq!(app.active_view(), WorkspaceView::Chat);
    assert_eq!(app.focus(), WorkspaceFocus::Nav);

    app.select_view(WorkspaceView::Git);
    assert_eq!(app.active_view(), WorkspaceView::Git);

    app.cycle_focus();
    assert_eq!(app.focus(), WorkspaceFocus::Content);

    app.cycle_focus();
    assert_eq!(app.focus(), WorkspaceFocus::Footer);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test chat_workspace workspace_switches_views_and_cycles_focus -- --exact`
Expected: FAIL with unresolved import or missing `WorkspaceApp`

- [ ] **Step 3: Write minimal workspace implementation**

```rust
use crate::chat::app::ChatApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceView {
    Chat,
    Git,
    Sessions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceFocus {
    Nav,
    Content,
    Footer,
}

pub struct WorkspaceApp {
    chat: ChatApp,
    active_view: WorkspaceView,
    focus: WorkspaceFocus,
}

impl WorkspaceApp {
    pub fn new_for_test(repo_path: &str) -> Self {
        Self {
            chat: ChatApp::new_for_test(repo_path),
            active_view: WorkspaceView::Chat,
            focus: WorkspaceFocus::Nav,
        }
    }

    pub fn active_view(&self) -> WorkspaceView {
        self.active_view
    }

    pub fn focus(&self) -> WorkspaceFocus {
        self.focus
    }

    pub fn select_view(&mut self, view: WorkspaceView) {
        self.active_view = view;
    }

    pub fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            WorkspaceFocus::Nav => WorkspaceFocus::Content,
            WorkspaceFocus::Content => WorkspaceFocus::Footer,
            WorkspaceFocus::Footer => WorkspaceFocus::Nav,
        };
    }

    pub fn chat(&self) -> &ChatApp {
        &self.chat
    }

    pub fn chat_mut(&mut self) -> &mut ChatApp {
        &mut self.chat
    }
}
```

- [ ] **Step 4: Export the workspace module**

```rust
pub mod app;
pub mod git;
pub mod input;
pub mod model;
pub mod session;
pub mod theme;
pub mod view;
pub mod workspace;
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --test chat_workspace workspace_switches_views_and_cycles_focus -- --exact`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/chat/mod.rs src/chat/workspace.rs tests/chat_workspace.rs
git commit -m "feat(chat): add workspace shell state"
```

## Task 2: Add workspace-level input actions

**Files:**
- Modify: `src/chat/input.rs`
- Modify: `src/chat/workspace.rs`
- Modify: `tests/chat_runtime.rs`
- Modify: `tests/chat_input_props.rs`

- [ ] **Step 1: Write the failing runtime shortcut test**

```rust
use blazar::chat::input::InputAction;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn digit_shortcuts_switch_workspace_views() {
    let chat = InputAction::from_key_event(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
    let git = InputAction::from_key_event(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
    let sessions =
        InputAction::from_key_event(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));

    assert!(matches!(chat, InputAction::SelectChatView));
    assert!(matches!(git, InputAction::SelectGitView));
    assert!(matches!(sessions, InputAction::SelectSessionsView));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test chat_runtime digit_shortcuts_switch_workspace_views -- --exact`
Expected: FAIL because the new variants do not exist yet

- [ ] **Step 3: Extend the input action enum**

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum InputAction {
    Quit,
    Submit,
    CycleFocus,
    SelectChatView,
    SelectGitView,
    SelectSessionsView,
    Key(KeyEvent),
}

impl InputAction {
    pub fn from_key_event(key: KeyEvent) -> Self {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => InputAction::Quit,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => InputAction::Quit,
            (KeyCode::Enter, _) => InputAction::Submit,
            (KeyCode::Tab, _) => InputAction::CycleFocus,
            (KeyCode::Char('1'), KeyModifiers::NONE) => InputAction::SelectChatView,
            (KeyCode::Char('2'), KeyModifiers::NONE) => InputAction::SelectGitView,
            (KeyCode::Char('3'), KeyModifiers::NONE) => InputAction::SelectSessionsView,
            _ => InputAction::Key(key),
        }
    }
}
```

- [ ] **Step 4: Route workspace-level actions**

```rust
pub fn handle_action(&mut self, action: InputAction) {
    match action {
        InputAction::CycleFocus => self.cycle_focus(),
        InputAction::SelectChatView => self.select_view(WorkspaceView::Chat),
        InputAction::SelectGitView => self.select_view(WorkspaceView::Git),
        InputAction::SelectSessionsView => self.select_view(WorkspaceView::Sessions),
        other => self.chat_mut().handle_action(other),
    }
}
```

- [ ] **Step 5: Run the focused shortcut tests**

Run: `cargo test --test chat_runtime digit_shortcuts_switch_workspace_views -- --exact && cargo test --test chat_input_props`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/chat/input.rs src/chat/workspace.rs tests/chat_runtime.rs tests/chat_input_props.rs
git commit -m "feat(chat): add workspace navigation shortcuts"
```

## Task 3: Render the workspace shell and updated chat view

**Files:**
- Modify: `src/chat/view.rs`
- Modify: `src/chat/theme.rs`
- Modify: `tests/chat_render.rs`
- Modify: `tests/chat_render_snapshot.rs`
- Modify: `tests/snapshots/chat_render_snapshot__default_chat_frame.snap`

- [ ] **Step 1: Write the failing shell render test**

```rust
use blazar::chat::view::render_workspace_to_lines_for_test;
use blazar::chat::workspace::WorkspaceApp;

#[test]
fn workspace_shell_shows_header_nav_and_chat_footer() {
    let app = WorkspaceApp::new_for_test(env!("CARGO_MANIFEST_DIR"));
    let lines = render_workspace_to_lines_for_test(&app, 100, 30);

    assert!(lines.iter().any(|line| line.contains("Blazar · Spirit Workspace")));
    assert!(lines.iter().any(|line| line.contains("Chat")));
    assert!(lines.iter().any(|line| line.contains("Git")));
    assert!(lines.iter().any(|line| line.contains("Sessions")));
    assert!(lines.iter().any(|line| line.contains("Ask Spirit")));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test chat_render workspace_shell_shows_header_nav_and_chat_footer -- --exact`
Expected: FAIL because the workspace render function does not exist yet

- [ ] **Step 3: Add workspace shell theme styles**

```rust
pub struct ChatTheme {
    pub shell_border: Style,
    pub rail_border: Style,
    pub panel_border: Style,
    pub active_nav: Style,
    pub inactive_nav: Style,
    pub status_text: Style,
    pub spirit_bubble: Style,
    pub user_bubble: Style,
}

pub fn build_theme() -> ChatTheme {
    ChatTheme {
        shell_border: Style::default().fg(Color::Cyan),
        rail_border: Style::default().fg(Color::Blue),
        panel_border: Style::default().fg(Color::Blue),
        active_nav: Style::default().fg(Color::Black).bg(Color::Cyan),
        inactive_nav: Style::default().fg(Color::Gray),
        status_text: Style::default().fg(Color::LightBlue),
        spirit_bubble: Style::default().fg(Color::White).bg(Color::Rgb(70, 40, 90)),
        user_bubble: Style::default().fg(Color::Black).bg(Color::Rgb(120, 210, 255)),
    }
}
```

- [ ] **Step 4: Replace the chat-only shell with a workspace render**

```rust
pub fn render_workspace_to_lines_for_test(
    app: &WorkspaceApp,
    width: u16,
    height: u16,
) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| render_workspace(frame, app, 1_200))
        .expect("workspace should render");
    extract_buffer_lines(terminal.backend().buffer(), width)
}
```

- [ ] **Step 5: Update snapshot coverage**

```rust
#[test]
fn default_chat_frame_snapshot() {
    let app = WorkspaceApp::new_for_test(env!("CARGO_MANIFEST_DIR"));
    let lines = render_workspace_to_lines_for_test(&app, 100, 30);

    assert_snapshot!("default_chat_frame", lines.join("\n"));
}
```

- [ ] **Step 6: Run render tests and snapshot update**

Run: `cargo test --test chat_render && INSTA_UPDATE=always cargo test --test chat_render_snapshot`
Expected: PASS with an updated snapshot showing the workspace shell

- [ ] **Step 7: Commit**

```bash
git add src/chat/view.rs src/chat/theme.rs tests/chat_render.rs tests/chat_render_snapshot.rs tests/snapshots/chat_render_snapshot__default_chat_frame.snap
git commit -m "feat(chat): render the spirit workspace shell"
```

## Task 4: Add the lightweight Git view

**Files:**
- Create: `src/chat/git.rs`
- Modify: `src/chat/workspace.rs`
- Modify: `src/chat/view.rs`
- Test: `tests/chat_git_view.rs`

- [ ] **Step 1: Write the failing Git view render test**

```rust
use blazar::chat::git::GitSummary;
use blazar::chat::view::render_workspace_to_lines_for_test;
use blazar::chat::workspace::{WorkspaceApp, WorkspaceView};

#[test]
fn git_view_shows_branch_status_and_recent_commits() {
    let mut app = WorkspaceApp::new_for_test(env!("CARGO_MANIFEST_DIR"));
    app.set_git_summary_for_test(GitSummary {
        branch: "feature/spirit-workspace".to_owned(),
        is_dirty: true,
        ahead: 2,
        behind: 0,
        staged: 1,
        unstaged: 3,
        changed_files: vec!["src/chat/view.rs".to_owned(), "tests/chat_render.rs".to_owned()],
        recent_commits: vec!["feat(chat): render the shell".to_owned()],
    });
    app.select_view(WorkspaceView::Git);

    let lines = render_workspace_to_lines_for_test(&app, 100, 30);

    assert!(lines.iter().any(|line| line.contains("feature/spirit-workspace")));
    assert!(lines.iter().any(|line| line.contains("dirty")));
    assert!(lines.iter().any(|line| line.contains("src/chat/view.rs")));
    assert!(lines.iter().any(|line| line.contains("feat(chat): render the shell")));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test chat_git_view git_view_shows_branch_status_and_recent_commits -- --exact`
Expected: FAIL because `GitSummary` and the Git pane render are missing

- [ ] **Step 3: Add the Git summary model**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSummary {
    pub branch: String,
    pub is_dirty: bool,
    pub ahead: usize,
    pub behind: usize,
    pub staged: usize,
    pub unstaged: usize,
    pub changed_files: Vec<String>,
    pub recent_commits: Vec<String>,
}

impl GitSummary {
    pub fn for_test() -> Self {
        Self {
            branch: "master".to_owned(),
            is_dirty: false,
            ahead: 0,
            behind: 0,
            staged: 0,
            unstaged: 0,
            changed_files: Vec::new(),
            recent_commits: vec!["No recent commits".to_owned()],
        }
    }
}
```

- [ ] **Step 4: Store Git summary in the workspace app**

```rust
pub struct WorkspaceApp {
    chat: ChatApp,
    active_view: WorkspaceView,
    focus: WorkspaceFocus,
    git_summary: GitSummary,
    session_summary: SessionSummary,
}
```

- [ ] **Step 5: Render the Git panel**

```rust
fn render_git_panel(frame: &mut Frame, area: Rect, app: &WorkspaceApp, theme: &ChatTheme) {
    let git = app.git_summary();
    let lines = vec![
        Line::from(format!("Branch: {}", git.branch)),
        Line::from(format!(
            "Status: {} · ↑{} ↓{} · staged {} · unstaged {}",
            if git.is_dirty { "dirty" } else { "clean" },
            git.ahead,
            git.behind,
            git.staged,
            git.unstaged
        )),
        Line::from(""),
        Line::from("Changed files"),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.panel_border)
        .title("Git");

    frame.render_widget(Paragraph::new(lines).block(block), area);
}
```

- [ ] **Step 6: Run the focused Git view test**

Run: `cargo test --test chat_git_view git_view_shows_branch_status_and_recent_commits -- --exact`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/chat/git.rs src/chat/workspace.rs src/chat/view.rs tests/chat_git_view.rs
git commit -m "feat(chat): add lightweight git view"
```

## Task 5: Add the Sessions view

**Files:**
- Create: `src/chat/session.rs`
- Modify: `src/chat/workspace.rs`
- Modify: `src/chat/view.rs`
- Test: `tests/chat_session_view.rs`

- [ ] **Step 1: Write the failing Sessions view test**

```rust
use blazar::chat::session::SessionSummary;
use blazar::chat::view::render_workspace_to_lines_for_test;
use blazar::chat::workspace::{WorkspaceApp, WorkspaceView};

#[test]
fn sessions_view_shows_current_plan_checkpoint_and_todo_counts() {
    let mut app = WorkspaceApp::new_for_test(env!("CARGO_MANIFEST_DIR"));
    app.set_session_summary_for_test(SessionSummary {
        session_label: "spirit-workspace".to_owned(),
        cwd: "/home/lx/blazar".to_owned(),
        active_intent: "Implementing workspace shell".to_owned(),
        plan_status: "plan.md present".to_owned(),
        checkpoints: vec!["Checkpoint 004".to_owned(), "Checkpoint 008".to_owned()],
        ready_todos: 2,
        in_progress_todos: 1,
        done_todos: 5,
    });
    app.select_view(WorkspaceView::Sessions);

    let lines = render_workspace_to_lines_for_test(&app, 100, 30);

    assert!(lines.iter().any(|line| line.contains("spirit-workspace")));
    assert!(lines.iter().any(|line| line.contains("plan.md present")));
    assert!(lines.iter().any(|line| line.contains("Checkpoint 004")));
    assert!(lines.iter().any(|line| line.contains("ready 2")));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test chat_session_view sessions_view_shows_current_plan_checkpoint_and_todo_counts -- --exact`
Expected: FAIL because `SessionSummary` and the Sessions pane are missing

- [ ] **Step 3: Add the Session summary model**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub session_label: String,
    pub cwd: String,
    pub active_intent: String,
    pub plan_status: String,
    pub checkpoints: Vec<String>,
    pub ready_todos: usize,
    pub in_progress_todos: usize,
    pub done_todos: usize,
}

impl SessionSummary {
    pub fn for_test() -> Self {
        Self {
            session_label: "current".to_owned(),
            cwd: env!("CARGO_MANIFEST_DIR").to_owned(),
            active_intent: "Ready".to_owned(),
            plan_status: "No plan loaded".to_owned(),
            checkpoints: vec!["No checkpoints recorded".to_owned()],
            ready_todos: 0,
            in_progress_todos: 0,
            done_todos: 0,
        }
    }
}
```

- [ ] **Step 4: Render the Sessions panel**

```rust
fn render_sessions_panel(
    frame: &mut Frame,
    area: Rect,
    app: &WorkspaceApp,
    theme: &ChatTheme,
) {
    let session = app.session_summary();
    let lines = vec![
        Line::from(format!("Session: {}", session.session_label)),
        Line::from(format!("Intent: {}", session.active_intent)),
        Line::from(format!("Plan: {}", session.plan_status)),
        Line::from(format!(
            "Todos: ready {} · in-progress {} · done {}",
            session.ready_todos, session.in_progress_todos, session.done_todos
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.panel_border)
        .title("Sessions");

    frame.render_widget(Paragraph::new(lines).block(block), area);
}
```

- [ ] **Step 5: Run the focused Sessions test**

Run: `cargo test --test chat_session_view sessions_view_shows_current_plan_checkpoint_and_todo_counts -- --exact`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/chat/session.rs src/chat/workspace.rs src/chat/view.rs tests/chat_session_view.rs
git commit -m "feat(chat): add sessions workspace view"
```

## Task 6: Add responsive fallback and runtime wiring

**Files:**
- Modify: `src/chat/app.rs`
- Modify: `src/chat/view.rs`
- Modify: `tests/chat_runtime.rs`
- Modify: `tests/chat_render.rs`

- [ ] **Step 1: Write the failing narrow-layout render test**

```rust
use blazar::chat::view::render_workspace_to_lines_for_test;
use blazar::chat::workspace::WorkspaceApp;

#[test]
fn workspace_collapses_to_single_column_on_narrow_widths() {
    let app = WorkspaceApp::new_for_test(env!("CARGO_MANIFEST_DIR"));
    let lines = render_workspace_to_lines_for_test(&app, 60, 20);

    assert!(lines.iter().any(|line| line.contains("Chat · Git · Sessions")));
    assert!(lines.iter().any(|line| line.contains("Ask Spirit")));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test chat_render workspace_collapses_to_single_column_on_narrow_widths -- --exact`
Expected: FAIL because there is no narrow-layout fallback yet

- [ ] **Step 3: Add width-aware shell routing**

```rust
pub fn render_workspace(frame: &mut Frame, app: &WorkspaceApp, tick_ms: u64) {
    let area = frame.area();
    if area.width < 80 {
        render_compact_workspace(frame, area, app, tick_ms);
    } else {
        render_wide_workspace(frame, area, app, tick_ms);
    }
}
```

- [ ] **Step 4: Run the terminal loop through `WorkspaceApp`**

```rust
let mut app = WorkspaceApp::new_for_test("");

loop {
    let tick_ms = start_time.elapsed().as_millis() as u64;
    terminal.draw(|frame| render_workspace(frame, &app, tick_ms))?;

    if event::poll(Duration::from_millis(100))?
        && let Event::Key(key) = event::read()?
    {
        let action = InputAction::from_key_event(key);
        app.handle_action(action);
    }

    if app.chat().should_quit() {
        break;
    }
}
```

- [ ] **Step 5: Run runtime and render coverage**

Run: `cargo test --test chat_render && cargo test --test chat_runtime && cargo test --test chat_workspace`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/chat/app.rs src/chat/view.rs tests/chat_render.rs tests/chat_runtime.rs tests/chat_workspace.rs
git commit -m "feat(chat): wire the spirit workspace runtime"
```

## Final verification

- [ ] Run `cargo test --test chat_boot`
- [ ] Run `cargo test --test chat_render`
- [ ] Run `cargo test --test chat_render_snapshot`
- [ ] Run `cargo test --test chat_git_view`
- [ ] Run `cargo test --test chat_session_view`
- [ ] Run `cargo test --test chat_runtime`
- [ ] Run `cargo test --test chat_workspace`
- [ ] Run `cargo test --test chat_input_props`
- [ ] Run `just fmt-check`
- [ ] Run `just lint`
- [ ] Run `just test`

## Spec coverage review

- **Chat as the main stage:** covered by Tasks 1, 3, and 6
- **First-class session management:** covered by Task 5
- **Lightweight Git surface:** covered by Task 4
- **Mascot reduced to support role:** covered by Task 3
- **Responsive fallback:** covered by Task 6
- **Keyboard-first navigation:** covered by Tasks 2 and 6
