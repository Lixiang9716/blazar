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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_text_zero_width_returns_full_string() {
        let result = wrap_text("hello world", 0);
        assert_eq!(result, vec!["hello world"]);
    }

    #[test]
    fn wrap_text_empty_string_returns_single_empty_chunk() {
        let result = wrap_text("", 10);
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn wrap_text_fits_within_width() {
        let result = wrap_text("hi", 10);
        assert_eq!(result, vec!["hi"]);
    }

    #[test]
    fn wrap_text_exact_width() {
        let result = wrap_text("abcde", 5);
        assert_eq!(result, vec!["abcde"]);
    }

    #[test]
    fn wrap_text_breaks_long_string() {
        let result = wrap_text("abcdef", 3);
        assert_eq!(result, vec!["abc", "def"]);
    }

    #[test]
    fn wrap_text_multi_byte_cjk_characters() {
        let result = wrap_text("你好世界", 4);
        assert_eq!(result, vec!["你好", "世界"]);
    }

    #[test]
    fn push_wrapped_lines_uses_prefix_and_indent() {
        let mut lines = Vec::new();
        let prefix = vec![Span::raw(MARGIN)];
        push_wrapped_lines(&mut lines, "abcdefghij", Style::default(), prefix, 8);
        assert!(lines.len() > 1, "should have multiple lines");
        let first_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first_text.starts_with(MARGIN));
        let second_text: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(second_text.starts_with(INDENT));
    }
}
