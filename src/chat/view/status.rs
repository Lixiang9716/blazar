use crate::chat::app::ChatApp;
use crate::chat::theme::ChatTheme;
use ratatui_core::{
    layout::Rect,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::paragraph::Paragraph;

pub(super) fn render_separator(frame: &mut Frame, area: Rect, theme: &ChatTheme) {
    let line = Line::from(Span::styled(
        "─".repeat(area.width as usize),
        theme.dim_text,
    ));
    let bar = Paragraph::new(line);
    frame.render_widget(bar, area);
}

pub(super) fn render_status_bar(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let left = "/ commands · ? help";

    let status = app.status_label();
    let status_style = if app.is_streaming() {
        theme.spinner
    } else if app.is_failed() {
        theme.marker_warning
    } else {
        theme.status_right
    };

    // Show short model name (last path segment) for brevity.
    let model_short = app
        .model_name()
        .rsplit('/')
        .next()
        .unwrap_or(app.model_name());
    let right = format!("{model_short} · {status}");

    let available = area.width as usize;
    let gap = available.saturating_sub(left.len() + right.len());

    let line = Line::from(vec![
        Span::styled(left, theme.status_bar),
        Span::styled(" ".repeat(gap), theme.status_bar),
        Span::styled(right, status_style),
    ]);

    let bar = Paragraph::new(line).style(theme.status_bar);
    frame.render_widget(bar, area);
}
