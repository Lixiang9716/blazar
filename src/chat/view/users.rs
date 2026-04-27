mod input_panel;
mod model_panel;
mod panels;
mod top_panel;

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

pub(super) fn users_area_height(
    total_height: u16,
    policy: UsersLayoutPolicy,
    app: &ChatApp,
) -> u16 {
    let base_height = policy.users_area_height(total_height);
    if !app.is_users_command_list_mode() {
        return base_height;
    }

    let expanded_height = policy
        .input_height
        .saturating_add(policy.model_height)
        .saturating_add(policy.max_command_window_size)
        .saturating_add(3);
    expanded_height.min(total_height.saturating_sub(1))
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
    let context = UsersPanelRenderContext { app, theme, policy };
    let registry = UsersPanelRenderRegistry::default();

    // Keep input/model visible first, then allocate separators around the input
    // row when there is enough room; any leftover height stays with top.
    let input_h = policy.input_height.min(area.height);
    let model_h = policy.model_height.min(area.height.saturating_sub(input_h));
    let separator_input_model_h = u16::from(
        input_h > 0
            && model_h > 0
            && area.height >= input_h.saturating_add(model_h).saturating_add(1),
    );
    let remaining_after_input_model = area.height.saturating_sub(
        input_h
            .saturating_add(model_h)
            .saturating_add(separator_input_model_h),
    );
    let separator_top_input_h = u16::from(input_h > 0 && remaining_after_input_model >= 2);
    let top_capacity = remaining_after_input_model.saturating_sub(separator_top_input_h);
    let top_h = if app.is_users_command_list_mode() {
        top_capacity
    } else {
        policy.top_height.min(top_capacity)
    };

    let mut y = area.y;
    let top_area = Rect::new(area.x, y, area.width, top_h);
    y = y.saturating_add(top_h);
    let separator_top_input_area = Rect::new(area.x, y, area.width, separator_top_input_h);
    y = y.saturating_add(separator_top_input_h);
    let input_area = Rect::new(area.x, y, area.width, input_h);
    y = y.saturating_add(input_h);
    let separator_input_model_area = Rect::new(area.x, y, area.width, separator_input_model_h);
    y = y.saturating_add(separator_input_model_h);
    let model_area = Rect::new(area.x, y, area.width, model_h);

    if top_area.height > 0 {
        registry.render(UsersPanelKind::Top, frame, top_area, &context);
    }
    if input_area.height > 0 {
        registry.render(UsersPanelKind::Input, frame, input_area, &context);
    }
    if separator_top_input_area.height > 0 {
        render_separator(frame, separator_top_input_area, theme);
    }
    if separator_input_model_area.height > 0 {
        render_separator(frame, separator_input_model_area, theme);
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
