use crate::chat::app::ChatApp;
use crate::chat::theme::ChatTheme;
use crate::chat::users_state::UsersLayoutPolicy;
use super::{input_panel::InputPanelRenderer, model_panel::ModelPanelRenderer};
use ratatui_core::{layout::Rect, terminal::Frame};

pub(super) struct UsersPanelRenderContext<'a> {
    pub app: &'a ChatApp,
    pub theme: &'a ChatTheme,
    pub policy: UsersLayoutPolicy,
}

pub(super) enum UsersPanelKind {
    Top,
    Input,
    Model,
}

pub(super) trait UsersPanelRenderer {
    fn supports(&self, kind: &UsersPanelKind) -> bool;
    fn render(&self, frame: &mut Frame, area: Rect, context: &UsersPanelRenderContext<'_>);
}

pub(super) struct UsersPanelRenderRegistry {
    top: TopPanelRenderer,
    input: InputPanelRenderer,
    model: ModelPanelRenderer,
}

impl Default for UsersPanelRenderRegistry {
    fn default() -> Self {
        Self {
            top: TopPanelRenderer,
            input: InputPanelRenderer,
            model: ModelPanelRenderer,
        }
    }
}

impl UsersPanelRenderRegistry {
    pub(super) fn render(
        &self,
        kind: UsersPanelKind,
        frame: &mut Frame,
        area: Rect,
        context: &UsersPanelRenderContext<'_>,
    ) {
        match kind {
            UsersPanelKind::Top => self.top.render(frame, area, context),
            UsersPanelKind::Input => self.input.render(frame, area, context),
            UsersPanelKind::Model => self.model.render(frame, area, context),
        }
    }
}

struct TopPanelRenderer;

impl UsersPanelRenderer for TopPanelRenderer {
    fn supports(&self, kind: &UsersPanelKind) -> bool {
        matches!(kind, UsersPanelKind::Top)
    }

    fn render(&self, frame: &mut Frame, area: Rect, context: &UsersPanelRenderContext<'_>) {
        super::top_panel::render_top_panel(frame, area, context.app, context.theme, context.policy);
    }
}
