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
use ratatui_widgets::paragraph::Paragraph;

const USERS_LAYOUT_POLICY: UsersLayoutPolicy = UsersLayoutPolicy {
    top_height: 1,
    input_height: 1,
    model_height: 1,
    max_command_window_size: 6,
};

pub(super) fn users_area_height(total_height: u16, policy: UsersLayoutPolicy) -> u16 {
    policy.users_area_height(total_height)
}

pub(super) fn render_users_area(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    render_users_area_with_policy(frame, area, app, theme, USERS_LAYOUT_POLICY);
}

pub(super) fn render_users_area_with_policy(
    frame: &mut Frame,
    area: Rect,
    app: &ChatApp,
    theme: &ChatTheme,
    policy: UsersLayoutPolicy,
) {
    let context = UsersPanelRenderContext { app, theme };
    let registry = UsersPanelRenderRegistry::default();

    let model_h = policy.model_height.min(area.height);
    let remaining_after_model = area.height.saturating_sub(model_h);
    let separator_h = u16::from(model_h > 0 && remaining_after_model > 0);
    let remaining_after_separator = remaining_after_model.saturating_sub(separator_h);
    let input_h = policy.input_height.min(remaining_after_separator);
    let remaining_after_input = remaining_after_separator.saturating_sub(input_h);
    let top_h = policy.top_height.min(remaining_after_input);

    let mut y = area.y;
    let top_area = Rect::new(area.x, y, area.width, top_h);
    y = y.saturating_add(top_h);
    let input_area = Rect::new(area.x, y, area.width, input_h);
    y = y.saturating_add(input_h);
    let separator_area = Rect::new(area.x, y, area.width, separator_h);
    y = y.saturating_add(separator_h);
    let model_area = Rect::new(area.x, y, area.width, model_h);

    if top_area.height > 0 {
        registry.render(UsersPanelKind::Top, frame, top_area, &context);
    }
    if input_area.height > 0 {
        registry.render(UsersPanelKind::Input, frame, input_area, &context);
    }
    if separator_area.height > 0 {
        render_separator(frame, separator_area, theme);
    }
    if model_area.height > 0 {
        registry.render(UsersPanelKind::Model, frame, model_area, &context);
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
