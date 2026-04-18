use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::Paragraph,
};

use crate::tui::state::FormState;

use super::components::{
    render_body, render_composite_overlay, render_footer, render_help_overlay, render_popup,
};

pub struct UiContext<'a> {
    pub status_message: &'a str,
    pub dirty: bool,
    pub error_count: usize,
    pub help: Option<&'a str>,
    pub global_errors: &'a [String],
    pub focus_label: Option<String>,
    pub session_title: Option<&'a str>,
    pub header_lines: Option<&'a [Line<'static>]>,
    pub popup: Option<PopupRender<'a>>,
    pub composite_overlay: Option<CompositeOverlay>,
    pub help_overlay: Option<HelpOverlayRender<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpShortcutRender {
    pub scope: String,
    pub keys: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpErrorRender {
    pub index: usize,
    pub pointer: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct HelpOverlayPage {
    pub summary: String,
    pub current_page: usize,
    pub total_pages: usize,
    pub shortcuts: Vec<HelpShortcutRender>,
    pub errors: Vec<HelpErrorRender>,
    pub total_errors: usize,
}

pub struct PopupRender<'a> {
    pub title: &'a str,
    pub options: &'a [String],
    pub selected: usize,
    pub multi: bool,
    pub active: Option<&'a [bool]>,
}

#[derive(Debug, Clone, Copy)]
pub struct HelpOverlayRender<'a> {
    pub page: &'a HelpOverlayPage,
    pub shortcut_offset: usize,
    pub error_offset: usize,
}

#[derive(Debug, Clone)]
pub struct CompositeOverlay {
    pub title: String,
    pub description: Option<String>,
    pub dirty: bool,
    pub instructions: String,
    pub list_entries: Option<Vec<String>>,
    pub list_selected: Option<usize>,
    pub entry_label: Option<String>,
    pub level: usize,
}

pub fn draw(
    frame: &mut Frame<'_>,
    form_state: &mut FormState,
    overlay_form: Option<&mut FormState>,
    ctx: UiContext<'_>,
) {
    let header_lines = ctx.header_lines.filter(|lines| !lines.is_empty());
    let header_height = header_lines.map(|lines| lines.len() as u16).unwrap_or(0);
    let chunks = if header_height > 0 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_height),
                Constraint::Min(7),
                Constraint::Length(4),
            ])
            .split(frame.area())
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(7), Constraint::Length(4)])
            .split(frame.area())
    };

    let body_index = if let Some(header_lines) = header_lines {
        let header = Paragraph::new(header_lines.to_vec());
        frame.render_widget(header, chunks[0]);
        1
    } else {
        0
    };
    let footer_index = body_index + 1;
    let content_area = Rect {
        x: frame.area().x,
        y: chunks[body_index].y,
        width: frame.area().width,
        height: frame.area().bottom().saturating_sub(chunks[body_index].y),
    };

    let cursor_enabled = ctx.popup.is_none() && ctx.composite_overlay.is_none();
    render_body(frame, chunks[body_index], form_state, cursor_enabled);
    render_footer(frame, chunks[footer_index], &ctx);

    // When both an overlay and a popup are present, render the overlay first
    // and the popup last so that the popup always appears on top. Drawing the
    // overlay after the popup would clear and cover the popup contents.
    if let (Some(meta), Some(overlay_state)) = (ctx.composite_overlay.as_ref(), overlay_form) {
        render_composite_overlay(frame, meta, overlay_state);
    }

    if let Some(popup) = ctx.popup {
        render_popup(frame, popup);
    }

    if let Some(help) = ctx.help_overlay {
        render_help_overlay(frame, content_area, help);
    }
}
