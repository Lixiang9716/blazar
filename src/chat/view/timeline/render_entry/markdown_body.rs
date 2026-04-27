use super::super::markdown::{MdSegment, normalize_markdown_paragraphs, split_code_fences};
use super::fenced_code::render_fenced_code;
use super::*;

#[derive(Clone, Copy)]
enum MarkdownTextMode {
    NormalizeParagraphs,
    PreserveLines,
}

pub(super) fn render_markdown_block<'a>(
    text: &str,
    theme: &ChatTheme,
    width: u16,
    first_prefix: Vec<Span<'a>>,
    cont_prefix: Vec<Span<'a>>,
) -> Vec<Line<'a>> {
    render_markdown_block_with_mode_and_text_style(
        text,
        theme,
        width,
        first_prefix,
        cont_prefix,
        MarkdownTextMode::NormalizeParagraphs,
        None,
    )
}

pub(super) fn render_markdown_block_with_text_style<'a>(
    text: &str,
    theme: &ChatTheme,
    width: u16,
    first_prefix: Vec<Span<'a>>,
    cont_prefix: Vec<Span<'a>>,
    text_style: Style,
) -> Vec<Line<'a>> {
    render_markdown_block_with_mode_and_text_style(
        text,
        theme,
        width,
        first_prefix,
        cont_prefix,
        MarkdownTextMode::NormalizeParagraphs,
        Some(text_style),
    )
}

pub(super) fn render_markdown_block_preserve_lines<'a>(
    text: &str,
    theme: &ChatTheme,
    width: u16,
    first_prefix: Vec<Span<'a>>,
    cont_prefix: Vec<Span<'a>>,
) -> Vec<Line<'a>> {
    render_markdown_block_with_mode_and_text_style(
        text,
        theme,
        width,
        first_prefix,
        cont_prefix,
        MarkdownTextMode::PreserveLines,
        None,
    )
}

fn render_markdown_block_with_mode_and_text_style<'a>(
    text: &str,
    theme: &ChatTheme,
    width: u16,
    first_prefix: Vec<Span<'a>>,
    cont_prefix: Vec<Span<'a>>,
    text_mode: MarkdownTextMode,
    text_style_override: Option<Style>,
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
                let rendered_text = match text_mode {
                    MarkdownTextMode::NormalizeParagraphs => {
                        let trimmed_seg = text.trim();
                        if trimmed_seg.is_empty() {
                            continue;
                        }
                        normalize_markdown_paragraphs(trimmed_seg)
                    }
                    MarkdownTextMode::PreserveLines => {
                        let trimmed_seg = text.trim_matches('\n');
                        if trimmed_seg.trim().is_empty() {
                            continue;
                        }
                        trimmed_seg.to_owned()
                    }
                };
                if rendered_text.is_empty() {
                    continue;
                }
                let md_lines =
                    rat_skin.parse(ratskin::RatSkin::parse_text(&rendered_text), text_width);
                for mut md_line in md_lines {
                    if let Some(style) = text_style_override {
                        for span in &mut md_line.spans {
                            span.style = style;
                        }
                    }
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
                let code_lines = render_fenced_code(lang, code, theme, text_width);
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
    render_markdown_block_preserve_lines(text, theme, width, prefix.clone(), prefix)
}
