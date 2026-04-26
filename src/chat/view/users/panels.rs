use crate::chat::app::ChatApp;
use crate::chat::theme::ChatTheme;
use crate::chat::users_state::UsersLayoutPolicy;
use crate::chat::view::{input, status};
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
    renderers: Vec<Box<dyn UsersPanelRenderer>>,
}

impl Default for UsersPanelRenderRegistry {
    fn default() -> Self {
        Self {
            renderers: vec![
                Box::new(TopPanelRenderer),
                Box::new(InputPanelRenderer),
                Box::new(ModelPanelRenderer),
            ],
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
        if let Some(renderer) = self
            .renderers
            .iter()
            .find(|renderer| renderer.supports(&kind))
        {
            renderer.render(frame, area, context);
        }
    }
}

struct TopPanelRenderer;
struct InputPanelRenderer;
struct ModelPanelRenderer;

impl UsersPanelRenderer for TopPanelRenderer {
    fn supports(&self, kind: &UsersPanelKind) -> bool {
        matches!(kind, UsersPanelKind::Top)
    }

    fn render(&self, frame: &mut Frame, area: Rect, context: &UsersPanelRenderContext<'_>) {
        let _ = context.policy.max_command_window_size;
        status::render_users_status_row(frame, area, context.app, context.theme);
    }
}

impl UsersPanelRenderer for InputPanelRenderer {
    fn supports(&self, kind: &UsersPanelKind) -> bool {
        matches!(kind, UsersPanelKind::Input)
    }

    fn render(&self, frame: &mut Frame, area: Rect, context: &UsersPanelRenderContext<'_>) {
        input::render_input(frame, area, context.app, context.theme);
    }
}

impl UsersPanelRenderer for ModelPanelRenderer {
    fn supports(&self, kind: &UsersPanelKind) -> bool {
        matches!(kind, UsersPanelKind::Model)
    }

    fn render(&self, frame: &mut Frame, area: Rect, context: &UsersPanelRenderContext<'_>) {
        status::render_mode_config_row(frame, area, context.app, context.theme);
    }
}
