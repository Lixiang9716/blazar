use super::*;

/// Break `text` into chunks that each fit within `max_cols` display columns.
pub(super) fn wrap_text(text: &str, max_cols: u16) -> Vec<String> {
    if max_cols == 0 {
        return vec![text.to_owned()];
    }
    let max = max_cols as usize;
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut col = 0usize;

    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if col + w > max && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            col = 0;
        }
        current.push(ch);
        col += w;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() {
        chunks.push(String::new());
    }
    chunks
}

/// Emit wrapped Lines: first line gets `prefix` spans, continuations get INDENT.
pub(super) fn push_wrapped_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    text: &str,
    style: Style,
    prefix: Vec<Span<'a>>,
    max_width: u16,
) {
    // Prefix display width: MARGIN(2) + marker(2) = 4 typically
    let text_width = max_width.saturating_sub(INDENT_WIDTH);
    let chunks = wrap_text(text, text_width);

    for (i, chunk) in chunks.into_iter().enumerate() {
        if i == 0 {
            let mut spans = prefix.clone();
            spans.push(Span::styled(chunk, style));
            lines.push(Line::from(spans));
        } else {
            lines.push(Line::from(vec![
                Span::raw(INDENT),
                Span::styled(chunk, style),
            ]));
        }
    }
}
