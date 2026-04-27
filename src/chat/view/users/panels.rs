use crate::chat::theme::ChatTheme;
use ratatui_core::{
    layout::Rect,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::paragraph::Paragraph;

pub(in crate::chat::view) fn render_separator(frame: &mut Frame, area: Rect, theme: &ChatTheme) {
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
