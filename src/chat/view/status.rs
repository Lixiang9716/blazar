use crate::chat::app::ChatApp;
use crate::chat::theme::ChatTheme;
use ratatui_core::{
    layout::Rect,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::paragraph::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::chat::users_state::UserMode;

// Shared behavior contract for the users model panel.
pub(super) fn render_mode_config_row(
    frame: &mut Frame,
    area: Rect,
    app: &ChatApp,
    theme: &ChatTheme,
) {
    let snapshot = app.users_status_snapshot();
    let left = match snapshot.mode {
        UserMode::Auto => "AUTO",
        UserMode::Plan => "PLAN",
    };

    let context_text = match snapshot.context_usage {
        Some(context) if context.max_tokens > 0 => format!(
            "{}/{} ({}%)",
            context.used_tokens,
            context.max_tokens,
            context.used_tokens.saturating_mul(100) / context.max_tokens
        ),
        _ => "n/a".to_owned(),
    };
    let debug = app.debug_status_label();
    let right = if debug.is_empty() {
        format!("{} · ctx {context_text}", snapshot.model_name)
    } else {
        format!("{} · ctx {context_text} · {debug}", snapshot.model_name)
    };

    let available = area.width as usize;
    let left_len = left.width();
    let right_len = right.width();
    let gap = available.saturating_sub(left_len + right_len);

    let line = Line::from(vec![
        Span::styled(left, theme.bold_text),
        Span::styled(" ".repeat(gap), theme.status_bar),
        Span::styled(right, theme.status_right),
    ]);

    let bar = Paragraph::new(line).style(theme.status_bar);
    frame.render_widget(bar, area);
}

#[cfg(test)]
mod tests {
    #[test]
    fn slash_query_normalization_replaces_crlf_with_spaces() {
        let normalized = crate::chat::app::normalize_slash_query("/help\nnext\r\nfinal");
        assert_eq!(normalized, "/help next final");
    }
}
