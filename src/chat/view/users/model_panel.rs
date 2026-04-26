use crate::chat::view::status;
use ratatui_core::{layout::Rect, terminal::Frame};

use super::panels::{UsersPanelRenderContext, UsersPanelRenderer};

pub(super) struct ModelPanelRenderer;

impl UsersPanelRenderer for ModelPanelRenderer {
    fn render(&self, frame: &mut Frame, area: Rect, context: &UsersPanelRenderContext<'_>) {
        status::render_mode_config_row(frame, area, context.app, context.theme);
    }
}
