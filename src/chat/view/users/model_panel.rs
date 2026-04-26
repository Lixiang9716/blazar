use crate::chat::view::status;
use ratatui_core::{layout::Rect, terminal::Frame};

use super::panels::{UsersPanelKind, UsersPanelRenderContext, UsersPanelRenderer};

pub(super) struct ModelPanelRenderer;

impl UsersPanelRenderer for ModelPanelRenderer {
    fn supports(&self, kind: &UsersPanelKind) -> bool {
        matches!(kind, UsersPanelKind::Model)
    }

    fn render(&self, frame: &mut Frame, area: Rect, context: &UsersPanelRenderContext<'_>) {
        status::render_mode_config_row(frame, area, context.app, context.theme);
    }
}
