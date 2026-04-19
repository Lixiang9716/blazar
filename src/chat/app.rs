use crate::chat::input::InputAction;
use crate::chat::model::{Author, ChatMessage, TimelineEntry};
use crate::chat::picker::ModalPicker;
use crate::config::MascotConfig;
use ratatui_textarea::TextArea;
use serde_json::Value;
use std::cell::Cell;
use std::time::Instant;

pub struct ChatApp {
    messages: Vec<ChatMessage>,
    timeline: Vec<TimelineEntry>,
    composer: TextArea<'static>,
    should_quit: bool,
    display_path: String,
    branch: String,
    scroll_offset: u16,
    show_details: bool,
    pub picker: ModalPicker,
    tick_count: u64,
    /// Remaining demo entries to play back.
    demo_queue: Vec<TimelineEntry>,
    /// When the last demo entry was added (for 1-second pacing).
    demo_last_add: Option<Instant>,
    /// Last known content height of the timeline (set by renderer).
    pub timeline_content_height: Cell<u16>,
    /// Last known visible height of the timeline area (set by renderer).
    pub timeline_visible_height: Cell<u16>,
}

impl ChatApp {
    pub fn new(repo_path: &str) -> Self {
        let display_path = shorten_home(repo_path);
        let branch = detect_branch(repo_path);

        let timeline = if std::env::var("BLAZAR_DEMO").is_ok() {
            demo_timeline()
        } else {
            vec![TimelineEntry::response(
                "Tell me what you'd like to explore.",
            )]
        };

        Self {
            messages: vec![ChatMessage {
                author: Author::Spirit,
                body: "Spirit: Tell me what you'd like to explore.".to_owned(),
            }],
            timeline,
            composer: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(ratatui_core::style::Style::default());
                ta
            },
            should_quit: false,
            display_path,
            branch,
            scroll_offset: u16::MAX, // auto-scroll sentinel
            show_details: false,
            picker: ModalPicker::command_palette(),
            tick_count: 0,
            demo_queue: Vec::new(),
            demo_last_add: None,
            timeline_content_height: Cell::new(0),
            timeline_visible_height: Cell::new(0),
        }
    }

    pub fn new_for_test(_repo_path: &str) -> Self {
        let mut app = Self::new(_repo_path);
        // Use a fixed display path so snapshots are environment-independent.
        app.display_path = "~/blazar".to_owned();
        app
    }

    /// Creates a ChatApp pre-loaded with demo timeline entries for visual testing.
    #[cfg(test)]
    pub fn new_with_demo_timeline(repo_path: &str) -> Self {
        let mut app = Self::new(repo_path);
        app.timeline = demo_timeline();
        app
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn timeline(&self) -> &[TimelineEntry] {
        &self.timeline
    }

    pub fn display_path(&self) -> &str {
        &self.display_path
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    pub fn status_label(&self) -> String {
        "ready".to_owned()
    }

    pub fn show_details(&self) -> bool {
        self.show_details
    }

    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        self.picker
            .overlay_state_mut()
            .tick(std::time::Duration::from_millis(100));

        // Demo playback: add one entry per second
        if !self.demo_queue.is_empty() {
            let should_add = match self.demo_last_add {
                Some(last) => last.elapsed().as_secs() >= 1,
                None => true,
            };
            if should_add {
                let entry = self.demo_queue.remove(0);
                self.timeline.push(entry);
                self.scroll_offset = u16::MAX; // auto-scroll
                self.demo_last_add = Some(Instant::now());
            }
        }
    }

    /// Whether demo playback is currently running.
    pub fn demo_active(&self) -> bool {
        !self.demo_queue.is_empty()
    }

    /// Convert the u16::MAX auto-scroll sentinel into a real offset
    /// so that manual scroll adjustments work correctly.
    fn resolve_scroll_sentinel(&mut self) {
        if self.scroll_offset == u16::MAX {
            self.scroll_offset = self
                .timeline_content_height
                .get()
                .saturating_sub(self.timeline_visible_height.get());
        }
    }

    pub fn send_message(&mut self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }

        // Trigger demo playback when user types "1"
        if trimmed == "1" {
            self.timeline.clear();
            self.demo_queue = demo_playback_script();
            self.demo_last_add = None;
            self.scroll_offset = u16::MAX;
            return;
        }

        self.messages.push(ChatMessage {
            author: Author::User,
            body: trimmed.to_owned(),
        });
        self.messages.push(ChatMessage {
            author: Author::Spirit,
            body: format!("Spirit: I hear you — {trimmed}"),
        });

        // Also append to timeline
        self.timeline.push(TimelineEntry::user_message(trimmed));
        self.timeline
            .push(TimelineEntry::response(format!("I hear you — {trimmed}")));

        // Auto-scroll to bottom
        self.scroll_offset = u16::MAX;
    }

    pub fn set_composer_text(&mut self, value: &str) {
        self.composer = TextArea::from([value.to_owned()]);
    }

    pub fn composer_text(&self) -> String {
        self.composer.lines().join("\n")
    }

    pub fn submit_composer(&mut self) {
        let text = self.composer_text();
        self.send_message(&text);
        self.composer = TextArea::default();
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn handle_action(&mut self, action: InputAction) {
        // When picker is open, route input to it
        if self.picker.is_visible() {
            match action {
                InputAction::Quit => self.picker.close(),
                InputAction::Submit => {
                    if let Some(cmd) = self.picker.select_current() {
                        self.picker.close();
                        self.send_message(&cmd);
                    }
                }
                InputAction::ScrollUp => self.picker.move_up(),
                InputAction::ScrollDown => self.picker.move_down(),
                InputAction::PickerUp => self.picker.move_up(),
                InputAction::PickerDown => self.picker.move_down(),
                InputAction::Backspace => {
                    if self.picker.filter.is_empty() {
                        self.picker.close();
                    } else {
                        self.picker.pop_filter();
                    }
                }
                InputAction::Key(key) => {
                    if let crossterm::event::KeyCode::Char(ch) = key.code {
                        self.picker.push_filter(ch);
                    }
                }
                _ => {}
            }
            return;
        }

        match action {
            InputAction::Quit => self.should_quit = true,
            InputAction::Submit => self.submit_composer(),
            InputAction::ToggleDetails => self.show_details = !self.show_details,
            InputAction::ScrollUp => {
                self.resolve_scroll_sentinel();
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
            }
            InputAction::ScrollDown => {
                self.resolve_scroll_sentinel();
                self.scroll_offset = self.scroll_offset.saturating_add(3);
            }
            InputAction::Key(key) => {
                // Open command palette when typing '/' in empty composer
                if let crossterm::event::KeyCode::Char('/') = key.code
                    && self.composer_text().is_empty()
                {
                    self.picker.open();
                    return;
                }
                self.composer.input(key);
            }
            InputAction::Backspace => {
                self.composer.input(crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Backspace,
                    crossterm::event::KeyModifiers::NONE,
                ));
            }
            _ => {}
        }
    }

    pub fn composer_mut(&mut self) -> &mut TextArea<'static> {
        &mut self.composer
    }

    pub fn composer(&self) -> &TextArea<'static> {
        &self.composer
    }
}

/// Shorten `/home/<user>/...` to `~/...` for display.
fn shorten_home(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME")
        && let Some(rest) = path.strip_prefix(&home)
    {
        return format!("~{rest}");
    }
    path.to_owned()
}

/// Detect the current git branch. Returns "main" as fallback.
fn detect_branch(repo_path: &str) -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_owned())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "main".to_owned())
}

/// Demo timeline entries for visual testing (BLAZAR_DEMO env var).
fn demo_timeline() -> Vec<TimelineEntry> {
    demo_playback_script().into_iter().take(3).collect()
}

/// Full demo playback script — one entry per second when triggered by "1".
/// Covers every entry kind: hint, warning, message, user_message,
/// tool_use (with diff details), bash, thinking, code_block.
fn demo_playback_script() -> Vec<TimelineEntry> {
    vec![
        // --- System initialization ---
        TimelineEntry::hint(
            "No blazar instructions found. Run /init to generate a blazar-instructions.md file.",
        ),
        TimelineEntry::warning("Failed to load 2 skills. Run /skills for more details."),
        TimelineEntry::response(
            "Environment loaded: 1 MCP server, 4 plugins, 12 skills, 3 agents",
        ),

        // --- User starts a task ---
        TimelineEntry::user_message("Fix the login page — the submit button doesn't work on mobile."),

        // --- Assistant thinks ---
        TimelineEntry::thinking(
            "The user wants to fix the mobile submit button on the login page. \
             I should look at the login component and its CSS first, \
             then check for touch event handlers.",
        ),

        // --- Assistant responds with analysis ---
        TimelineEntry::response(
            "I'll investigate the login page. Let me check the component and its styles.",
        ),

        // --- Read file ---
        TimelineEntry::tool_use(
            "Read",
            "src/components/LoginForm.tsx",
            0,
            0,
            "Reading login form component",
        )
        .with_details(
            "export function LoginForm() {\n\
             \x20 const [email, setEmail] = useState('');\n\
             \x20 const handleSubmit = (e: MouseEvent) => {\n\
             \x20   e.preventDefault();\n\
             \x20   submitLogin(email, password);\n\
             \x20 };\n\
             \x20 return (\n\
             \x20   <button onClick={handleSubmit}>Sign In</button>\n\
             \x20 );\n\
             }",
        ),

        // --- Bash: run tests ---
        TimelineEntry::bash(
            "npm test -- --grep 'LoginForm'",
            "FAIL src/components/LoginForm.test.tsx\n  ✕ submit fires on touch (24ms)\n  ✓ renders email input (3ms)\n  ✓ shows validation error (5ms)",
        )
        .with_details(
            "$ npm test -- --grep 'LoginForm'\n\n\
             FAIL src/components/LoginForm.test.tsx\n\
             \x20 ● submit fires on touch\n\
             \x20   expect(mockSubmit).toHaveBeenCalled()\n\
             \x20   Expected: called\n\
             \x20   Received: not called\n\n\
             Tests: 1 failed, 2 passed, 3 total\n\
             Time:  1.842s",
        ),

        // --- Assistant analyzes ---
        TimelineEntry::response(
            "Found the bug: `onClick` only fires on mouse click, not touch. \
             Mobile Safari requires `onPointerDown` or a combined handler. \
             I'll fix the event handler and update the CSS for touch targets.",
        ),

        // --- Edit file (with diff) ---
        TimelineEntry::tool_use(
            "Edit",
            "src/components/LoginForm.tsx",
            5,
            3,
            "Switch onClick to onPointerDown for mobile support",
        )
        .with_details(
            "  const handleSubmit = (e: MouseEvent) => {\n\
             -   e.preventDefault();\n\
             -   submitLogin(email, password);\n\
             - };\n\
             + const handleSubmit = (e: React.PointerEvent | React.MouseEvent) => {\n\
             +   e.preventDefault();\n\
             +   submitLogin(email, password);\n\
             + };\n\
             \n\
             - <button onClick={handleSubmit}>Sign In</button>\n\
             + <button onPointerDown={handleSubmit} style={{ touchAction: 'manipulation' }}>\n\
             +   Sign In\n\
             + </button>",
        ),

        // --- Edit CSS ---
        TimelineEntry::tool_use(
            "Edit",
            "src/styles/login.css",
            8,
            0,
            "Add mobile touch-friendly button styles",
        )
        .with_details(
            "+ .login-btn {\n\
             +   min-height: 44px;\n\
             +   min-width: 44px;\n\
             +   touch-action: manipulation;\n\
             +   -webkit-tap-highlight-color: transparent;\n\
             +   cursor: pointer;\n\
             +   padding: 12px 24px;\n\
             +   font-size: 16px; /* prevents iOS zoom */\n\
             + }",
        ),

        // --- Run tests again ---
        TimelineEntry::bash(
            "npm test -- --grep 'LoginForm'",
            "PASS src/components/LoginForm.test.tsx\n  ✓ submit fires on touch (8ms)\n  ✓ renders email input (3ms)\n  ✓ shows validation error (4ms)",
        )
        .with_details(
            "$ npm test -- --grep 'LoginForm'\n\n\
             PASS src/components/LoginForm.test.tsx\n\
             \x20 ✓ submit fires on touch (8ms)\n\
             \x20 ✓ renders email input (3ms)\n\
             \x20 ✓ shows validation error (4ms)\n\n\
             Tests: 3 passed, 3 total\n\
             Time:  1.204s",
        ),

        // --- Code block ---
        TimelineEntry::code_block(
            "typescript",
            "// Key change: onPointerDown works on both mouse and touch\n\
             <button\n\
             \x20 onPointerDown={handleSubmit}\n\
             \x20 style={{ touchAction: 'manipulation' }}\n\
             >\n\
             \x20 Sign In\n\
             </button>",
        ),

        // --- Lint check ---
        TimelineEntry::bash(
            "npx eslint src/components/LoginForm.tsx",
            "No lint errors found.",
        ),

        // --- Summary response with markdown ---
        TimelineEntry::response(
            "**Fixed.** The mobile submit button now works correctly.\n\n\
             ### Changes\n\
             - Replaced `onClick` with `onPointerDown` for cross-device support\n\
             - Added `touchAction: 'manipulation'` to prevent 300ms delay\n\
             - CSS: minimum 44×44px touch target (WCAG 2.5.5)\n\
             - Font size 16px to prevent iOS auto-zoom\n\n\
             All 3 tests pass. The fix covers iOS Safari, Android Chrome, \
             and desktop browsers.",
        ),

        // --- Another user request ---
        TimelineEntry::user_message("Can you also add a loading spinner to the button?"),

        // --- Thinking ---
        TimelineEntry::thinking(
            "The user wants a loading state. I'll add a spinner component \
             that shows during the async login request, disabling the button \
             to prevent double-submit.",
        ),

        // --- Tool use: create new file ---
        TimelineEntry::tool_use(
            "Create",
            "src/components/Spinner.tsx",
            12,
            0,
            "Create reusable spinner component",
        )
        .with_details(
            "+ import React from 'react';\n\
             + import './spinner.css';\n\
             +\n\
             + interface SpinnerProps {\n\
             +   size?: number;\n\
             +   color?: string;\n\
             + }\n\
             +\n\
             + export function Spinner({ size = 16, color = 'white' }: SpinnerProps) {\n\
             +   return <span className=\"spinner\" style={{ width: size, height: size, borderColor: color }} />;\n\
             + }",
        ),

        // --- Final summary ---
        TimelineEntry::response(
            "**Done.** Added a `<Spinner>` component that shows during login. \
             The button is disabled while loading to prevent double-submits.\n\n\
             Run `/commit` to stage these changes.",
        ),
    ]
}

pub fn run_terminal_chat(
    schema: Value,
    _mascot: MascotConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::chat::view::render_frame;
    use crossterm::{
        ExecutableCommand,
        event::{self, EnableMouseCapture, Event, MouseEventKind},
        terminal::{EnterAlternateScreen, enable_raw_mode},
    };
    use ratatui_core::terminal::Terminal;
    use ratatui_crossterm::CrosstermBackend;
    use std::io::stdout;
    use std::time::{Duration, Instant};

    let repo_path = resolve_repo_path(&schema);
    let mut app = ChatApp::new(&repo_path);

    // Setup terminal; the guard ensures cleanup on any exit path including `?`.
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    stdout().execute(EnableMouseCapture)?;
    let _guard = TerminalGuard;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let start_time = Instant::now();

    // Event loop
    loop {
        let tick_ms = start_time.elapsed().as_millis() as u64;
        app.tick();

        terminal.draw(|frame| render_frame(frame, &mut app, tick_ms))?;

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    let action = InputAction::from_key_event(key);
                    app.handle_action(action);
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        app.handle_action(InputAction::ScrollUp);
                    }
                    MouseEventKind::ScrollDown => {
                        app.handle_action(InputAction::ScrollDown);
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        if app.should_quit() {
            break;
        }
    }

    Ok(())
    // _guard drops here, restoring raw mode and alternate screen.
}

/// Restores raw mode and alternate screen when dropped.
///
/// Ensures `disable_raw_mode` and `LeaveAlternateScreen` are always called
/// even when `run_terminal_chat` returns early via a `?`-propagated error.
pub(crate) struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        use crossterm::ExecutableCommand;
        use crossterm::event::DisableMouseCapture;
        let _ = std::io::stdout().execute(DisableMouseCapture);
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = std::io::stdout().execute(crossterm::terminal::LeaveAlternateScreen);
    }
}

/// Extracts the repository path from the schema JSON, falling back to the
/// current working directory.  Extracted as a standalone function so it can be
/// unit-tested without running the terminal event loop.
pub fn resolve_repo_path(schema: &Value) -> String {
    schema
        .pointer("/properties/workspace/properties/repoPath/default")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        })
}
