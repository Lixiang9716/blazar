//! Terminal event loop and lifecycle management.

use crate::chat::app::ChatApp;
use crate::chat::input::InputAction;
use crate::config::MascotConfig;
use serde_json::Value;

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
