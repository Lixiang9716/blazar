/// Markdown segment: either plain text or a fenced code block.
pub(super) enum MdSegment {
    Text(String),
    Code { lang: String, body: String },
}

/// Split markdown into text and fenced-code segments.
/// Unfenced content (including indented code blocks) stays as Text.
pub(super) fn split_code_fences(src: &str) -> Vec<MdSegment> {
    let mut segments = Vec::new();
    let mut text_buf = String::new();
    let mut lines = src.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            // Flush accumulated text
            if !text_buf.is_empty() {
                segments.push(MdSegment::Text(std::mem::take(&mut text_buf)));
            }
            let lang = trimmed.trim_start_matches('`').trim().to_owned();
            let mut code_buf = String::new();
            let mut closed = false;

            for code_line in lines.by_ref() {
                if code_line.trim().starts_with("```") {
                    closed = true;
                    break;
                }
                if !code_buf.is_empty() {
                    code_buf.push('\n');
                }
                code_buf.push_str(code_line);
            }

            if closed {
                segments.push(MdSegment::Code {
                    lang,
                    body: code_buf,
                });
            } else {
                // Unclosed fence — treat original lines as text
                text_buf.push_str("```");
                text_buf.push_str(&lang);
                text_buf.push('\n');
                text_buf.push_str(&code_buf);
            }
        } else {
            if !text_buf.is_empty() {
                text_buf.push('\n');
            }
            text_buf.push_str(line);
        }
    }

    if !text_buf.is_empty() {
        segments.push(MdSegment::Text(text_buf));
    }

    // Guarantee at least one segment
    if segments.is_empty() {
        segments.push(MdSegment::Text(String::new()));
    }

    segments
}

/// Join soft line-breaks within paragraphs so termimad doesn't treat them as
/// hard breaks.  Preserves structural markdown elements (headings, lists,
/// code fences, tables, blank lines) and code-block interiors.
pub(super) fn normalize_markdown_paragraphs(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let lines: Vec<&str> = text.split('\n').collect();
    let mut in_code_block = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
        }

        result.push_str(line);

        if i + 1 < lines.len() {
            if in_code_block {
                result.push('\n');
                continue;
            }
            let next_trimmed = lines[i + 1].trim();
            if is_structural_line(trimmed) || is_structural_line(next_trimmed) {
                result.push('\n');
            } else {
                result.push(' ');
            }
        }
    }
    result
}

fn is_structural_line(s: &str) -> bool {
    s.is_empty()
        || s.starts_with('#')
        || s.starts_with("- ")
        || s.starts_with("* ")
        || s.starts_with("+ ")
        || s.starts_with("```")
        || s.starts_with("> ")
        || s.starts_with('|')
        || s.starts_with("---")
        || s.starts_with("===")
        || s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() && s.contains(". "))
}
