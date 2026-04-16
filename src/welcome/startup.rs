use crate::welcome::state::WelcomeState;
use crate::welcome::view::render_scene;

#[derive(Debug)]
pub struct WelcomeController {
    state: WelcomeState,
}

impl WelcomeController {
    pub fn new() -> Self {
        Self {
            state: WelcomeState::new(),
        }
    }

    pub fn frame(&mut self, now_ms: u64, input_buffer: &str) -> String {
        let has_input = !input_buffer.trim().is_empty();
        self.state = self.state.tick(now_ms, has_input);
        render_scene(self.state)
    }
}
