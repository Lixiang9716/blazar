use crate::chat::app::ChatApp;
use crate::chat::theme::ChatTheme;
use ratatui_core::{
    layout::Rect,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_macros::horizontal;
use ratatui_widgets::paragraph::Paragraph;

use ratatui_core::layout::Position;

// Shared behavior contract for the users input panel.
pub(super) fn render_input(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let [prompt_area, composer_area] = horizontal![==2, >=1].areas(area);

    let prompt =
        Paragraph::new(Line::from(Span::styled("> ", theme.input_prompt))).style(theme.timeline_bg);
    frame.render_widget(prompt, prompt_area);

    if app.composer_text().is_empty() {
        let placeholder = Paragraph::new(Line::from(Span::styled(
            "Describe a task or ask a question…",
            theme.input_placeholder,
        )))
        .style(theme.timeline_bg);
        frame.render_widget(placeholder, composer_area);
    } else {
        let composer = app.composer();
        frame.render_widget(composer, composer_area);
    }

    // Place the terminal cursor at the composer position so that IME
    // popups (e.g. Chinese/Japanese input) appear in the right place.
    if composer_area.width > 0 && composer_area.height > 0 {
        let sc = app.composer().screen_cursor();
        let max_col = composer_area.width.saturating_sub(1) as usize;
        let max_row = composer_area.height.saturating_sub(1) as usize;
        frame.set_cursor_position(Position::new(
            composer_area.x + sc.col.min(max_col) as u16,
            composer_area.y + sc.row.min(max_row) as u16,
        ));
    }
}
