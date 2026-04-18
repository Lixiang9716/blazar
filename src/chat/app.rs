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
    pub fn new_for_test(_repo_path: &str) -> Self {
        Self {
            messages: vec![ChatMessage {
                author: Author::Spirit,
                body: "Spirit: Tell me what you'd like to explore.".to_owned(),
            }],
            composer: TextArea::default(),
            should_quit: false,
        }
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
    use crate::chat::view::render_frame;
    use crossterm::{
        event::{self, Event},
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
        ExecutableCommand,
    };
    use ratatui_core::terminal::Terminal;
    use ratatui_crossterm::CrosstermBackend;
    use std::io::stdout;
    use std::time::{Duration, Instant};

    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    // Initialize app
    let mut app = ChatApp::new_for_test("");
    let start_time = Instant::now();

    // Event loop
    loop {
        let tick_ms = start_time.elapsed().as_millis() as u64;

        // Render
        terminal.draw(|frame| render_frame(frame, &app, tick_ms))?;

        // Handle events with timeout
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let action = InputAction::from_key_event(key);
                app.handle_action(action);
            }
        }

        // Check quit
        if app.should_quit() {
            break;
        }
    }

    // Cleanup
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    Ok(())
}
