use crate::chat::app::ChatApp;
use crate::chat::theme::ChatTheme;
use ratatui_core::{
    layout::Rect,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::paragraph::Paragraph;

const SPARKLE_SPINNER: &[&str] = &["✶", "✸", "✹", "✺", "✹", "✸"];

/// Render a single-line streaming indicator: sparkle spinner + status label.
pub(super) fn render_streaming_indicator(
    frame: &mut Frame,
    area: Rect,
    tick_ms: u64,
    app: &ChatApp,
    theme: &ChatTheme,
) {
    if area.height == 0 || area.width < 4 {
        return;
    }

    let idx = ((tick_ms / 120) % SPARKLE_SPINNER.len() as u64) as usize;
    let sparkle = SPARKLE_SPINNER[idx];

    let label = Line::from(vec![
        Span::styled(format!(" {sparkle} "), theme.spinner),
        Span::styled(app.status_label(), theme.dim_text),
    ]);

    let bar = Paragraph::new(label);
    frame.render_widget(bar, area);
}
