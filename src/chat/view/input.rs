use crate::chat::app::ChatApp;
use crate::chat::theme::ChatTheme;
use ratatui_core::{
    layout::Rect,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_macros::horizontal;
use ratatui_widgets::paragraph::Paragraph;

pub(super) fn render_input(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let [prompt_area, composer_area] = horizontal![==2, >=1].areas(area);

    let prompt =
        Paragraph::new(Line::from(Span::styled("› ", theme.input_prompt))).style(theme.timeline_bg);
    frame.render_widget(prompt, prompt_area);

    if app.composer_text().is_empty() {
        // Empty — show nothing, just the cursor position
        let empty = Paragraph::new("").style(theme.timeline_bg);
        frame.render_widget(empty, composer_area);
    } else {
        let composer = app.composer();
        frame.render_widget(composer, composer_area);
    }
}
