use super::super::markdown::{MdSegment, normalize_markdown_paragraphs, split_code_fences};
use super::super::text_wrap::push_wrapped_lines;
use super::fenced_code::render_fenced_code;
use super::*;

pub(super) fn render_message_entry<'a>(
    entry: &TimelineEntry,
    theme: &ChatTheme,
    width: u16,
    marker_style: Style,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    if !entry.body.is_empty() {
        if entry.actor == Actor::User {
            // User messages: `›` prefix, plain bold text (no markdown)
            for (i, body_line) in entry.body.lines().enumerate() {
                let prefix = if i == 0 {
                    vec![Span::raw(MARGIN), Span::styled("› ", marker_style)]
                } else {
                    vec![Span::raw(INDENT)]
                };
                push_wrapped_lines(&mut lines, body_line, theme.bold_text, prefix, width);
            }
        } else {
            // Assistant messages: render markdown via ratskin (termimad backend)
            // Normalize soft breaks first — termimad treats every \n as hard break.
            let body = entry.body.trim();
            if body.is_empty() {
                return lines;
            } else {
                // Assistant messages: split code fences for custom rendering,
                // render remaining text via ratskin (termimad backend).
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
                            let md_lines = rat_skin
                                .parse(ratskin::RatSkin::parse_text(&normalized), text_width);
                            for md_line in md_lines {
                                let prefix = if is_first_line {
                                    is_first_line = false;
                                    vec![Span::raw(MARGIN), Span::styled("● ", marker_style)]
                                } else {
                                    vec![Span::raw(INDENT)]
                                };
                                let mut result_spans = prefix;
                                result_spans.extend(md_line.spans);
                                lines.push(Line::from(result_spans));
                            }
                        }
                        MdSegment::Code { lang, body: code } => {
                            if is_first_line {
                                is_first_line = false;
                                lines.push(Line::from(vec![
                                    Span::raw(MARGIN),
                                    Span::styled("● ", marker_style),
                                ]));
                            }
                            let code_lines = render_fenced_code(lang, code, theme, text_width);
                            for code_line in code_lines {
                                let mut result_spans = vec![Span::raw(INDENT)];
                                result_spans.extend(code_line.spans);
                                lines.push(Line::from(result_spans));
                            }
                        }
                    }
                }
            }
        }
    } else {
        return lines;
    }

    lines
}
