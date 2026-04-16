use std::io::{self, BufRead, Write};

use crate::welcome::state::WelcomeState;
use crate::welcome::view::render_scene;

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
        render_scene(self.state)
    }
}

pub fn run_session<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> io::Result<()> {
    let mut welcome = WelcomeController::new();
    writeln!(output, "{}", welcome.frame(0, ""))?;

    let mut line = String::new();
    input.read_line(&mut line)?;

    writeln!(output, "{}", welcome.frame(1_500, &line))?;
    Ok(())
}

impl Default for WelcomeController {
    fn default() -> Self {
        Self::new()
    }
}
