use crate::chat::view::input;
use ratatui_core::{layout::Rect, terminal::Frame};

use super::panels::{UsersPanelKind, UsersPanelRenderContext, UsersPanelRenderer};

pub(super) struct InputPanelRenderer;

impl UsersPanelRenderer for InputPanelRenderer {
    fn supports(&self, kind: &UsersPanelKind) -> bool {
        matches!(kind, UsersPanelKind::Input)
    }

    fn render(&self, frame: &mut Frame, area: Rect, context: &UsersPanelRenderContext<'_>) {
        input::render_input(frame, area, context.app, context.theme);
    }
}
