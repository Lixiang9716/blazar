use crate::chat::input::InputAction;
use crate::chat::model::{Author, ChatMessage, TimelineEntry};
use crate::chat::picker::ModalPicker;
use crate::config::MascotConfig;
use ratatui_textarea::TextArea;
use serde_json::Value;

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
    }

    pub fn send_message(&mut self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
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
        if self.picker.visible {
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
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
            }
            InputAction::ScrollDown => {
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

/// Demo timeline entries for visual testing.
/// Activated by setting `BLAZAR_DEMO=1` environment variable.
fn demo_timeline() -> Vec<TimelineEntry> {
    vec![
        TimelineEntry::hint(
            "No blazar instructions found. Run /init to generate a blazar-instructions.md file.",
        ),
        TimelineEntry::warning("Failed to load 2 skills. Run /skills for more details."),
        TimelineEntry::response("Environment loaded: 1 MCP server, 4 plugins, 12 skills, 3 agents"),
        TimelineEntry::tool_use(
            "Edit",
            "src/chat/view.rs",
            18,
            6,
            "Replaced hard guard with stacked layout branch",
        )
        .with_details(
            " 63    let [title, timeline, input, status] =\n\
             -64        vertical![==1, >=1, ==3, ==1].areas(area);\n\
             +64        vertical![==6, >=1, ==1, ==3, ==1].areas(area);\n\
             \n\
             src/chat/view.rs",
        ),
        TimelineEntry::bash("cargo test --lib", "77 tests passed, 0 failed (2.4s)").with_details(
            "cd /home/lx/blazar && cargo test --lib 2>&1\n\
                 running 77 tests\n\
                 test chat_render ... ok\n\
                 test welcome_view ... ok\n\
                 ...\n\
                 test result: ok. 77 passed; 0 failed (2.4s)",
        ),
        TimelineEntry::response(
            "**Fixed.** Narrow terminals (< 60 cols) now get a stacked layout:\n\n- Row 1: mascot (3 lines, centered)\n- Row 2: chat timeline (remaining space)\n- Row 3: `input` + `status`\n\n77 tests pass including the new `narrow-render` snapshot.",
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
        event::{self, Event},
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
    let _guard = TerminalGuard;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let start_time = Instant::now();

    // Event loop
    loop {
        let tick_ms = start_time.elapsed().as_millis() as u64;
        app.tick();

        terminal.draw(|frame| render_frame(frame, &app, tick_ms))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            let action = InputAction::from_key_event(key);
            app.handle_action(action);
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
