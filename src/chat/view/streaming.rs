use crate::chat::theme::ChatTheme;
use ratatui_core::{
    layout::Rect,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::paragraph::Paragraph;

const BRAILLE_SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Render a single-line streaming indicator: braille spinner + "streaming…".
pub(super) fn render_streaming_indicator(
    frame: &mut Frame,
    area: Rect,
    tick_ms: u64,
    theme: &ChatTheme,
) {
    if area.height == 0 || area.width < 4 {
        return;
    }

    let idx = ((tick_ms / 80) % BRAILLE_SPINNER.len() as u64) as usize;
    let spinner_char = BRAILLE_SPINNER[idx];

    let label = Line::from(vec![
        Span::styled(format!(" {spinner_char} "), theme.spinner),
        Span::styled("streaming…", theme.dim_text),
    ]);

    let bar = Paragraph::new(label);
    frame.render_widget(bar, area);
}
