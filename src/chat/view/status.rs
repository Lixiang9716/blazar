use crate::chat::app::ChatApp;
use crate::chat::theme::ChatTheme;
use ratatui_core::{
    layout::Rect,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::paragraph::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::chat::users_state::{StatusMode, UserMode, UsersStatusSnapshot};

pub(super) fn render_users_status_row(
    frame: &mut Frame,
    area: Rect,
    app: &ChatApp,
    theme: &ChatTheme,
) {
    let snapshot = app.users_status_snapshot();
    if snapshot.status_mode == StatusMode::CommandList {
        let query = app.normalized_slash_query();
        let command_matches = app.inline_command_matches();
        let status_text = if command_matches.is_empty() {
            format!("{query} · No command matches")
        } else {
            format!("{query} · {}", command_matches.join("  "))
        };
        let line = Line::from(Span::styled(
            truncate_left_status_text(&status_text, area.width as usize),
            theme.status_bar,
        ));
        let bar = Paragraph::new(line).style(theme.status_bar);
        frame.render_widget(bar, area);
        return;
    }

    let left = format_status_left(&snapshot);
    let refs_summary = format_references_summary(&snapshot.referenced_files);

    let status = app.status_label();
    let status_style = if app.is_streaming() {
        theme.spinner
    } else if app.is_failed() {
        theme.marker_warning
    } else {
        theme.status_right
    };

    let debug = app.debug_status_label();
    let right = if debug.is_empty() {
        format!("{refs_summary} · {status}")
    } else {
        format!("{refs_summary} · {status} · {debug}")
    };

    let available = area.width as usize;
    let right_len = right.width();

    // Truncate left side if total exceeds available width
    let max_left = available.saturating_sub(right_len + 1);
    let left_display = truncate_left_status_text(&left, max_left);

    let gap = available.saturating_sub(left_display.width() + right_len);

    let line = Line::from(vec![
        Span::styled(left_display, theme.status_bar),
        Span::styled(" ".repeat(gap), theme.status_bar),
        Span::styled(right, status_style),
    ]);

    let bar = Paragraph::new(line).style(theme.status_bar);
    frame.render_widget(bar, area);
}

fn format_status_left(snapshot: &UsersStatusSnapshot) -> String {
    let mut left = snapshot.current_path.clone();
    if !snapshot.branch.is_empty() {
        left.push_str(&format!(" ({})", snapshot.branch));
    }
    if let Some(pr_label) = snapshot
        .pr_label
        .as_deref()
        .filter(|label| !label.is_empty())
    {
        left.push_str(&format!(" [{pr_label}]"));
    }
    left.push_str(" · / commands");
    left
}

fn format_references_summary(referenced_files: &[String]) -> String {
    let refs = referenced_files
        .iter()
        .filter_map(|entry| {
            let trimmed = entry.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .collect::<Vec<_>>();

    if refs.is_empty() {
        return "refs:-".to_owned();
    }

    let shown = refs.iter().take(2).copied().collect::<Vec<_>>();
    let hidden_count = refs.len().saturating_sub(shown.len());
    if hidden_count == 0 {
        format!("refs: {}", shown.join(", "))
    } else {
        format!("refs: {} +{hidden_count}", shown.join(", "))
    }
}

fn truncate_left_status_text(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_owned();
    }
    if max_width == 1 {
        return "…".to_owned();
    }

    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if used + width > max_width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += width;
    }
    out.push('…');
    out
}

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
    let right = format!("{} · ctx {context_text}", snapshot.model_name);

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
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn truncate_status_text_handles_multibyte_chars_without_panicking() {
        let original = "状态状态状态";
        let truncated = super::truncate_left_status_text(original, 5);
        assert_eq!(truncated, "状态…");
        assert_eq!(truncated.width(), 5);
    }

    #[test]
    fn truncate_status_text_returns_original_when_it_fits() {
        let original = "blazar";
        let truncated = super::truncate_left_status_text(original, 10);
        assert_eq!(truncated, "blazar");
    }

    #[test]
    fn slash_query_normalization_replaces_crlf_with_spaces() {
        let normalized = crate::chat::app::normalize_slash_query("/help\nnext\r\nfinal");
        assert_eq!(normalized, "/help next final");
    }
}
