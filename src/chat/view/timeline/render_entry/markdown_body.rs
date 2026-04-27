use super::super::markdown::{MdSegment, normalize_markdown_paragraphs, split_code_fences};
use super::*;

pub(super) fn render_markdown_block<'a>(
    text: &str,
    theme: &ChatTheme,
    width: u16,
    first_prefix: Vec<Span<'a>>,
    cont_prefix: Vec<Span<'a>>,
) -> Vec<Line<'a>> {
    let body = text.trim();
    if body.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let segments = split_code_fences(body);
    let rat_skin = ratskin::RatSkin {
        skin: theme.mad_skin.clone(),
    };
    let text_width = width.saturating_sub(INDENT_WIDTH);
    let mut is_first_line = true;

    for segment in &segments {
        match segment {
            MdSegment::Text(text) => {
                let trimmed_seg = text.trim();
                if trimmed_seg.is_empty() {
                    continue;
                }
                let normalized = normalize_markdown_paragraphs(trimmed_seg);
                let md_lines =
                    rat_skin.parse(ratskin::RatSkin::parse_text(&normalized), text_width);
                for md_line in md_lines {
                    let mut result_spans = if is_first_line {
                        is_first_line = false;
                        first_prefix.clone()
                    } else {
                        cont_prefix.clone()
                    };
                    result_spans.extend(md_line.spans);
                    lines.push(Line::from(result_spans));
                }
            }
            MdSegment::Code { lang, body: code } => {
                let code_lines = super::message::render_fenced_code(lang, code, theme, text_width);
                for code_line in code_lines {
                    let mut result_spans = if is_first_line {
                        is_first_line = false;
                        first_prefix.clone()
                    } else {
                        cont_prefix.clone()
                    };
                    result_spans.extend(code_line.spans);
                    lines.push(Line::from(result_spans));
                }
            }
        }
    }

    lines
}

pub(in super::super) fn render_markdown_details_block<'a>(
    text: &str,
    theme: &ChatTheme,
    width: u16,
    prefix: Vec<Span<'a>>,
) -> Vec<Line<'a>> {
    render_markdown_block(text, theme, width, prefix.clone(), prefix)
}
