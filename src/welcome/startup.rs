use std::io::{self, BufRead, Write};
use std::thread;
use std::time::Duration;

use crate::welcome::state::WelcomeState;
use crate::welcome::view::render_scene;

const IDLE_AFTER_MS: u64 = 1_200;
const ANIMATION_SAMPLE_MS: u64 = 125;

pub struct WelcomeController {
    state: WelcomeState,
}

impl WelcomeController {
    pub fn new() -> Self {
        Self {
            state: WelcomeState::new(),
        }
    }

    pub fn frame(&mut self, now_ms: u64, input: &str) -> String {
        self.state = self.state.tick(now_ms, !input.trim().is_empty());
        render_scene(self.state, now_ms)
    }
}

pub fn run_session<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> io::Result<()> {
    let mut welcome = WelcomeController::new();
    writeln!(output, "{}", welcome.frame(0, ""))?;

    thread::sleep(Duration::from_millis(IDLE_AFTER_MS));
    writeln!(output, "{}", welcome.frame(IDLE_AFTER_MS, ""))?;

    thread::sleep(Duration::from_millis(ANIMATION_SAMPLE_MS));
    writeln!(
        output,
        "{}",
        welcome.frame(IDLE_AFTER_MS + ANIMATION_SAMPLE_MS, "")
    )?;

    let mut line = String::new();
    input.read_line(&mut line)?;

    if !line.trim().is_empty() {
        writeln!(
            output,
            "{}",
            welcome.frame(IDLE_AFTER_MS + ANIMATION_SAMPLE_MS, &line)
        )?;
    }

    Ok(())
}

impl Default for WelcomeController {
    fn default() -> Self {
        Self::new()
    }
}
