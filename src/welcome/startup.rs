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

impl Default for WelcomeController {
    fn default() -> Self {
        Self::new()
    }
}
