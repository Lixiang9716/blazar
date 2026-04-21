//! Terminal event loop and lifecycle management.

use crate::chat::app::ChatApp;
use crate::chat::input::InputAction;
use log::{debug, info, trace};
use serde_json::Value;

pub fn run_terminal_chat(schema: Value) -> Result<(), Box<dyn std::error::Error>> {
    use crate::chat::view::render_frame;
    use crossterm::{
        ExecutableCommand,
        event::{self, EnableBracketedPaste, EnableMouseCapture, Event, MouseEventKind},
        terminal::{EnterAlternateScreen, enable_raw_mode},
    };
    use ratatui_core::terminal::Terminal;
    use ratatui_crossterm::CrosstermBackend;
    use std::io::stdout;
    use std::time::{Duration, Instant};

    let repo_path = resolve_repo_path(&schema);
    info!("event_loop: repo_path={repo_path}");
    let mut app = ChatApp::new(&repo_path)?;

    // Setup terminal; the guard ensures cleanup on any exit path including `?`.
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    stdout().execute(EnableMouseCapture)?;
    // Bracketed paste: multi-line paste arrives as single Event::Paste(String).
    // Non-fatal — some terminals may not support it.
    if let Err(e) = stdout().execute(EnableBracketedPaste) {
        debug!("event_loop: bracketed paste not supported: {e}");
    }
    let _guard = TerminalGuard;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    info!("event_loop: terminal initialized");

    let start_time = Instant::now();

    // Event loop
    loop {
        let tick_ms = start_time.elapsed().as_millis() as u64;
        app.tick();

        terminal.draw(|frame| render_frame(frame, &mut app, tick_ms))?;

        // Use faster polling during animations for smooth ~30 FPS.
        let poll_timeout = if app.demo_active() {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(100)
        };

        if event::poll(poll_timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    let action = InputAction::from_key_event(key);
                    trace!(
                        "event_loop: key={:?} modifiers={:?} → action={:?}",
                        key.code, key.modifiers, action
                    );
                    app.handle_action(action);
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        trace!("event_loop: mouse scroll_up");
                        app.handle_action(InputAction::ScrollUp);
                    }
                    MouseEventKind::ScrollDown => {
                        trace!("event_loop: mouse scroll_down");
                        app.handle_action(InputAction::ScrollDown);
                    }
                    _ => {}
                },
                Event::Resize(w, h) => {
                    debug!("event_loop: terminal resized to {w}x{h}");
                }
                Event::Paste(text) => {
                    debug!("event_loop: paste len={}", text.len());
                    app.handle_action(InputAction::Paste(text));
                }
                _ => {}
            }
        }

        if app.should_quit() {
            info!("event_loop: quit requested");
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
        use crossterm::event::{DisableBracketedPaste, DisableMouseCapture};
        let _ = std::io::stdout().execute(DisableBracketedPaste);
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
