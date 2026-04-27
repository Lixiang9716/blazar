use super::*;

/// Render a fenced code block with language label, background, and borders.
pub(super) fn render_fenced_code<'a>(
    lang: &str,
    code: &str,
    theme: &ChatTheme,
    text_width: u16,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let w = text_width as usize;
    let code_style = Style::default()
        .fg(theme.code_block.fg.unwrap_or(Color::Reset))
        .bg(theme.code_bg);
    let border_style = theme.dim_text;

    let top = if !lang.is_empty() {
        let label = format!(" {lang} ");
        let label_w = UnicodeWidthStr::width(label.as_str());
        let bar_len = w.saturating_sub(label_w);
        format!("{label}{}", "─".repeat(bar_len))
    } else {
        "─".repeat(w)
    };
    lines.push(Line::from(Span::styled(top, border_style)));

    if code.is_empty() {
        lines.push(Line::from(Span::styled(" ".repeat(w), code_style)));
    } else {
        for code_line in code.lines() {
            let expanded = code_line.replace('\t', "    ");
            let display_w = UnicodeWidthStr::width(expanded.as_str());
            let padding = w.saturating_sub(display_w);
            lines.push(Line::from(Span::styled(
                format!("{expanded}{}", " ".repeat(padding)),
                code_style,
            )));
        }
    }

    lines.push(Line::from(Span::styled("─".repeat(w), border_style)));
    lines
}
