use crate::chat::view::input;
use ratatui_core::{layout::Rect, terminal::Frame};

use super::panels::{UsersPanelRenderContext, UsersPanelRenderer};

pub(super) struct InputPanelRenderer;

impl UsersPanelRenderer for InputPanelRenderer {
    fn render(&self, frame: &mut Frame, area: Rect, context: &UsersPanelRenderContext<'_>) {
        input::render_input(frame, area, context.app, context.theme);
    }
}
