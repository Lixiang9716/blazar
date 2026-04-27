//! Chat rendering — each sub-module handles one visual region.

pub mod render;

mod input;
mod picker;
mod status;
mod streaming;
mod timeline;
mod users;

use crate::chat::app::ChatApp;
use crate::chat::users_state::UsersLayoutPolicy;
use core::cmp;
use log::warn;
use ratatui_core::{
    backend::TestBackend,
    layout::Rect,
    terminal::{Frame, Terminal},
    text::{Line, Span},
};
use ratatui_macros::vertical;
use ratatui_widgets::block::Block;
use ratatui_widgets::paragraph::Paragraph;
use unicode_width::UnicodeWidthStr;

use self::render::contracts::{RenderCtx, RenderError, RenderRegistry, RenderSlot};
use self::render::registry::DefaultRenderRegistry;

pub fn render_to_lines_for_test(app: &mut ChatApp, width: u16, height: u16) -> Vec<String> {
    render_to_lines_for_test_with_users_policy(app, width, height, UsersLayoutPolicy::default())
}

pub fn render_to_lines_for_test_with_users_policy(
    app: &mut ChatApp,
    width: u16,
    height: u16,
    users_policy: UsersLayoutPolicy,
) -> Vec<String> {
    let registry = DefaultRenderRegistry::default();
    render_to_lines_for_test_with_registry(app, width, height, users_policy, &registry)
}

pub fn render_to_lines_for_test_with_registry(
    app: &mut ChatApp,
    width: u16,
    height: u16,
    users_policy: UsersLayoutPolicy,
    registry: &dyn RenderRegistry,
) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

    terminal
        .draw(|frame| render_frame_with_registry(frame, app, 1_200, users_policy, registry))
        .expect("chat frame should render");

    let buffer = terminal.backend().buffer();
    buffer
        .content()
        .chunks(width as usize)
        .map(|row| {
            let mut line = String::new();
            let mut skip = 0;

            for cell in row {
                if skip == 0 {
                    line.push_str(cell.symbol());
                }
                skip = cmp::max(skip, cell.symbol().width()).saturating_sub(1);
            }

            line
        })
        .collect()
}

pub fn render_frame(frame: &mut Frame, app: &mut ChatApp, tick_ms: u64) {
    let registry = DefaultRenderRegistry::default();
    render_frame_with_registry(frame, app, tick_ms, UsersLayoutPolicy::default(), &registry);
}

pub fn render_frame_with_users_policy(
    frame: &mut Frame,
    app: &mut ChatApp,
    tick_ms: u64,
    users_policy: UsersLayoutPolicy,
) {
    let registry = DefaultRenderRegistry::default();
    render_frame_with_registry(frame, app, tick_ms, users_policy, &registry);
}

fn render_frame_with_registry(
    frame: &mut Frame,
    app: &mut ChatApp,
    tick_ms: u64,
    users_policy: UsersLayoutPolicy,
    registry: &dyn RenderRegistry,
) {
    let theme = app.theme().clone();
    let area = frame.area();

    let bg_block = Block::default().style(theme.timeline_bg);
    frame.render_widget(bg_block, area);

    let streaming = app.is_streaming();
    let users_height = users::users_area_height(area.height, users_policy, app);
    let [timeline_area, users_area] = vertical![>=1, ==(users_height)].areas(area);
    let planned_user_slots = users::plan_users_slots(users_area, users_policy, app);
    let mut ctx = RenderCtx::new(app, theme, tick_ms, users_policy);

    render_slot_or_fallback(
        registry,
        RenderSlot::Timeline,
        frame,
        timeline_area,
        &mut ctx,
    );

    if streaming && timeline_area.height > 0 {
        let indicator_area = Rect::new(
            timeline_area.x,
            timeline_area
                .y
                .saturating_add(timeline_area.height.saturating_sub(1)),
            timeline_area.width,
            1,
        );
        streaming::render_streaming_indicator(
            frame,
            indicator_area,
            ctx.tick_ms(),
            ctx.app(),
            ctx.theme(),
        );
    }

    for slot in planned_user_slots {
        render_slot_or_fallback(registry, slot.kind, frame, slot.area, &mut ctx);
    }

    if ctx.app().picker.is_visible() {
        render_slot_or_fallback(registry, RenderSlot::PickerOverlay, frame, area, &mut ctx);
    }
}

fn render_slot_or_fallback(
    registry: &dyn RenderRegistry,
    slot: RenderSlot,
    frame: &mut Frame,
    area: Rect,
    ctx: &mut RenderCtx<'_>,
) {
    if let Err(err) = registry.render_slot(slot, frame, area, ctx) {
        warn!("chat view slot render failed: slot={slot:?} err={err:?}");
        render_slot_error_fallback(frame, area, ctx.theme(), &err);
    }
}

fn render_slot_error_fallback(
    frame: &mut Frame,
    area: Rect,
    theme: &crate::chat::theme::ChatTheme,
    err: &RenderError,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let message = match err {
        RenderError::RegistryMissingSlot(slot) => format!("render slot unavailable: {slot:?}"),
        RenderError::ComponentError(message) => format!("render error: {message}"),
    };
    let fallback = Paragraph::new(Line::from(Span::styled(message, theme.marker_warning)))
        .style(theme.timeline_bg);
    frame.render_widget(fallback, area);
}

pub(in crate::chat::view) fn render_picker_overlay_slot(
    frame: &mut Frame,
    area: Rect,
    ctx: &mut RenderCtx<'_>,
) {
    let theme = ctx.theme().clone();
    picker::render_picker(frame, area, ctx.app_mut(), &theme);
}
