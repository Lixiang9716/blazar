use crate::chat::input::InputAction;
use crate::chat::model::{Author, ChatMessage};
use crate::config::MascotConfig;
use ratatui_textarea::TextArea;
use serde_json::Value;

pub struct ChatApp {
    messages: Vec<ChatMessage>,
    composer: TextArea<'static>,
    should_quit: bool,
}

impl ChatApp {
    pub fn new(_repo_path: &str) -> Self {
        Self {
            messages: vec![ChatMessage {
                author: Author::Spirit,
                body: "Spirit: Tell me what you'd like to explore.".to_owned(),
            }],
            composer: TextArea::default(),
            should_quit: false,
        }
    }

    pub fn new_for_test(repo_path: &str) -> Self {
        Self::new(repo_path)
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
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
        match action {
            InputAction::Quit => self.should_quit = true,
            InputAction::Submit => self.submit_composer(),
            InputAction::CycleFocus
            | InputAction::SelectChatView
            | InputAction::SelectGitView
            | InputAction::SelectSessionsView => {}
            InputAction::Key(key) => {
                self.composer.input(key);
            }
        }
    }

    pub fn composer_mut(&mut self) -> &mut TextArea<'static> {
        &mut self.composer
    }

    pub fn composer(&self) -> &TextArea<'static> {
        &self.composer
    }
}

pub fn run_terminal_chat(
    _schema: Value,
    _mascot: MascotConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::chat::view::render_workspace;
    use crate::chat::workspace::WorkspaceApp;
    use crossterm::{
        ExecutableCommand,
        event::{self, Event},
        terminal::{EnterAlternateScreen, enable_raw_mode},
    };
    use ratatui_core::terminal::Terminal;
    use ratatui_crossterm::CrosstermBackend;
    use std::io::stdout;
    use std::time::{Duration, Instant};

    // Resolve repo path and initialise the app BEFORE touching the terminal so
    // that the potentially slow git/session I/O does not run inside raw mode.
    let repo_path = resolve_repo_path(&_schema);
    let mut app = WorkspaceApp::new(&repo_path);

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

        terminal.draw(|frame| render_workspace(frame, &app, tick_ms))?;

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
        let _ = std::io::stdout()
            .execute(crossterm::terminal::LeaveAlternateScreen);
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
