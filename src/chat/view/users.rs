mod panels;

use crate::chat::app::ChatApp;
use crate::chat::theme::ChatTheme;
use crate::chat::users_state::UsersLayoutPolicy;
use panels::{UsersPanelKind, UsersPanelRenderContext, UsersPanelRenderRegistry};
use ratatui_core::{
    layout::Rect,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_macros::vertical;
use ratatui_widgets::paragraph::Paragraph;

const USERS_LAYOUT_POLICY: UsersLayoutPolicy = UsersLayoutPolicy {
    top_height: 1,
    input_height: 1,
    model_height: 1,
    max_command_window_size: 6,
};

pub(super) fn users_area_height(total_height: u16) -> u16 {
    USERS_LAYOUT_POLICY.users_area_height(total_height)
}

pub(super) fn render_users_area(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let context = UsersPanelRenderContext {
        app,
        theme,
        policy: USERS_LAYOUT_POLICY,
    };
    let registry = UsersPanelRenderRegistry::default();

    match area.height {
        0 => {}
        1 => registry.render(UsersPanelKind::Top, frame, area, &context),
        2 => {
            let [top_area, model_area] = vertical![>=0, ==1].areas(area);
            registry.render(UsersPanelKind::Top, frame, top_area, &context);
            registry.render(UsersPanelKind::Model, frame, model_area, &context);
        }
        _ => {
            let [top_area, input_area, separator_area, model_area] =
                vertical![==1, ==1, ==1, ==1].areas(area);
            registry.render(UsersPanelKind::Top, frame, top_area, &context);
            registry.render(UsersPanelKind::Input, frame, input_area, &context);
            render_separator(frame, separator_area, theme);
            registry.render(UsersPanelKind::Model, frame, model_area, &context);
        }
    }
}

fn render_separator(frame: &mut Frame, area: Rect, theme: &ChatTheme) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let separator = Paragraph::new(Line::from(Span::styled(
        "─".repeat(area.width as usize),
        theme.status_bar,
    )))
    .style(theme.status_bar);
    frame.render_widget(separator, area);
}
