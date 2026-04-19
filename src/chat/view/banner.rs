use crate::chat::app::ChatApp;
use crate::chat::theme::ChatTheme;
use crate::welcome::mascot::render_mascot_lines;
use crate::welcome::state::WelcomeState;
use core::cmp;
use ratatui_core::{
    layout::Rect,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_macros::horizontal;
use ratatui_widgets::block::Block;
use ratatui_widgets::borders::BorderType;
use ratatui_widgets::paragraph::Paragraph;

pub(super) fn render_welcome_banner(
    frame: &mut Frame,
    area: Rect,
    _app: &ChatApp,
    tick_ms: u64,
    theme: &ChatTheme,
) {
    let version = env!("CARGO_PKG_VERSION");

    // Animate mascot: use real elapsed time so sprite frames cycle
    let mascot_lines = render_mascot_lines(WelcomeState::new(), tick_ms);
    let mascot_rows: Vec<Line<'static>> = mascot_lines
        .into_iter()
        .skip_while(|line| {
            line.width() == 0 || line.spans.iter().all(|s| s.content.trim().is_empty())
        })
        .take_while(|line| {
            line.width() > 0 && !line.spans.iter().all(|s| s.content.trim().is_empty())
        })
        .collect();
    let mascot_width = mascot_rows.first().map(|l| l.width()).unwrap_or(0) as u16;

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.dim_text);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 2 || inner.height < 2 {
        return;
    }

    // Split inner: mascot | text
    let mascot_col_width = cmp::min(mascot_width + 1, inner.width / 3);
    let [mascot_area, text_area] = horizontal![==(mascot_col_width), >=1].areas(inner);

    // Render mascot sprite — vertically centered in its area
    let mascot_h = mascot_rows.len() as u16;
    let y_offset = mascot_area.height.saturating_sub(mascot_h) / 2;
    let centered_mascot = Rect::new(
        mascot_area.x,
        mascot_area.y + y_offset,
        mascot_area.width,
        cmp::min(mascot_h, mascot_area.height),
    );
    let mascot_paragraph = Paragraph::new(mascot_rows);
    frame.render_widget(mascot_paragraph, centered_mascot);

    // Render text beside mascot — vertically centered
    let text_lines = vec![
        Line::from(vec![
            Span::styled("Blazar", theme.title_text),
            Span::styled(format!(" v{version}"), theme.dim_text),
        ]),
        Line::from(Span::styled(
            "Describe a task to get started.",
            theme.body_text,
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Tip: ", theme.dim_text),
            Span::styled("/help", theme.tip_command),
            Span::styled(
                " for commands. Blazar uses AI. Check for mistakes.",
                theme.dim_text,
            ),
        ]),
    ];
    let text_h = text_lines.len() as u16;
    let text_y_offset = text_area.height.saturating_sub(text_h) / 2;
    let centered_text = Rect::new(
        text_area.x,
        text_area.y + text_y_offset,
        text_area.width,
        cmp::min(text_h, text_area.height),
    );
    let text_paragraph = Paragraph::new(text_lines);
    frame.render_widget(text_paragraph, centered_text);
}
