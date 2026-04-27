use crate::chat::app::ChatApp;
use crate::chat::theme::ChatTheme;
use crate::chat::users_state::{StatusMode, UsersLayoutPolicy};
use ratatui_core::{
    layout::Rect,
    style::Style,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::paragraph::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(in crate::chat::view) fn render_top_panel(
    frame: &mut Frame,
    area: Rect,
    app: &ChatApp,
    theme: &ChatTheme,
    policy: UsersLayoutPolicy,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let snapshot = app.users_status_snapshot();
    let title = format_top_title(&snapshot.current_path, &snapshot.branch);

    render_row(
        frame,
        Rect::new(area.x, area.y, area.width, 1),
        &title,
        theme.status_bar,
    );

    if snapshot.status_mode != StatusMode::CommandList || area.height == 1 {
        return;
    }

    let available_rows = area.height.saturating_sub(1) as usize;
    let visible_rows = available_rows
        .min(policy.max_command_window_size as usize)
        .min(
            app.inline_command_matches()
                .len()
                .saturating_sub(app.users_command_scroll_offset()),
        );

    let command_count = app.inline_command_matches().len();
    let start = if command_count == 0 {
        0
    } else {
        app.users_command_scroll_offset()
            .min(command_count.saturating_sub(visible_rows))
    };
    let end = (start + visible_rows).min(command_count);
    let visible_commands = &app.inline_command_matches()[start..end];

    if visible_commands.is_empty() {
        render_row(
            frame,
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
            "No command matches",
            theme.status_bar,
        );
        return;
    }

    for (index, command) in visible_commands.iter().enumerate() {
        let y = area.y.saturating_add(1 + index as u16);
        if y >= area.y.saturating_add(area.height) {
            break;
        }
        render_row(
            frame,
            Rect::new(area.x, y, area.width, 1),
            &format!("• {command}"),
            theme.status_bar,
        );
    }
}

fn format_top_title(current_path: &str, branch: &str) -> String {
    if branch.is_empty() {
        current_path.to_owned()
    } else {
        format!("{current_path} · {branch}")
    }
}

fn render_row(frame: &mut Frame, area: Rect, text: &str, style: Style) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let line = Line::from(Span::styled(
        truncate_text(text, area.width as usize),
        style,
    ));
    frame.render_widget(Paragraph::new(line).style(style), area);
}

fn truncate_text(text: &str, max_width: usize) -> String {
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
