use crate::chat::app::ChatApp;
use crate::chat::model::{Actor, EntryKind, TimelineEntry};
use crate::chat::theme::{ChatTheme, build_theme};
use core::cmp;
use ratatui_core::{
    backend::TestBackend,
    layout::Rect,
    style::Style,
    terminal::{Frame, Terminal},
    text::{Line, Span},
};
use ratatui_macros::{horizontal, vertical};
use ratatui_widgets::{
    block::Block,
    paragraph::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

pub fn render_to_lines_for_test(app: &ChatApp, width: u16, height: u16) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

    terminal
        .draw(|frame| render_frame(frame, app, 1_200))
        .expect("chat frame should render");

    let buffer = terminal.backend().buffer();
    buffer
        .content()
        .chunks(width as usize)
        .map(|row| {
            let mut line = String::new();
            let mut skip = 0;

            for cell in row {
                if skip == 0 {
                    line.push_str(cell.symbol());
                }
                skip = cmp::max(skip, cell.symbol().width()).saturating_sub(1);
            }

            line
        })
        .collect()
}

pub fn render_frame(frame: &mut Frame, app: &ChatApp, _tick_ms: u64) {
    let theme = build_theme();
    let area = frame.area();

    // Fill background with Solarized Dark base03
    let bg_block = Block::default().style(theme.timeline_bg);
    frame.render_widget(bg_block, area);

    // Vertical layout: title_bar | timeline | input | status_bar
    let [title, timeline, input, status] = vertical![==1, >=1, ==3, ==1].areas(area);

    render_title_bar(frame, title, app, &theme);
    render_timeline(frame, timeline, app, &theme);
    render_input(frame, input, app, &theme);
    render_status_bar(frame, status, app, &theme);
}

fn render_title_bar(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let display_path = app.display_path();
    let title_text = format!("blazar — {display_path}");

    // Center the title in the bar
    let padding = area.width.saturating_sub(title_text.len() as u16) / 2;
    let padded = format!(
        "{:>width$}",
        title_text,
        width = (padding as usize) + title_text.len()
    );

    let line = Line::from(Span::styled(padded, theme.title_text));
    let bar = Paragraph::new(line).style(theme.title_bar);
    frame.render_widget(bar, area);
}

fn render_timeline(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let mut lines: Vec<Line> = Vec::new();

    for entry in app.timeline() {
        let entry_lines = render_entry(entry, theme, area.width);
        lines.extend(entry_lines);
        lines.push(Line::from("")); // blank separator
    }

    // If no entries, show welcome
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Welcome to Blazar. Type a message to begin.",
            theme.dim_text,
        )));
    }

    // Calculate scroll: auto-scroll to bottom
    let content_height = lines.len() as u16;
    let visible_height = area.height;
    let scroll_offset = if content_height > visible_height {
        let auto_scroll = content_height.saturating_sub(visible_height);
        // Respect manual scroll if set
        cmp::min(app.scroll_offset(), auto_scroll)
    } else {
        0
    };

    let paragraph = Paragraph::new(lines)
        .style(theme.timeline_bg)
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    frame.render_widget(paragraph, area);
}

/// Left margin for all timeline content (matches Claude Code's indentation).
const MARGIN: &str = "  ";
/// Continuation indent (margin + marker width).
const INDENT: &str = "    ";

fn render_entry<'a>(entry: &TimelineEntry, theme: &ChatTheme, _width: u16) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let marker_style = marker_style_for(entry, theme);

    match &entry.kind {
        EntryKind::Message => {
            if !entry.body.is_empty() {
                if entry.actor == Actor::User {
                    // User messages: plain bold text (no markdown)
                    let mut body_lines = entry.body.lines();
                    if let Some(first) = body_lines.next() {
                        lines.push(Line::from(vec![
                            Span::raw(MARGIN),
                            Span::styled("● ", marker_style),
                            Span::styled(first.to_owned(), theme.bold_text),
                        ]));
                    }
                    for cont in body_lines {
                        lines.push(Line::from(vec![
                            Span::raw(INDENT),
                            Span::styled(cont.to_owned(), theme.bold_text),
                        ]));
                    }
                } else {
                    // Assistant messages: render markdown
                    let md_text = tui_markdown::from_str(&entry.body);
                    for (i, md_line) in md_text.lines.into_iter().enumerate() {
                        let owned_spans: Vec<Span<'a>> = md_line
                            .spans
                            .into_iter()
                            .map(|s| Span::styled(s.content.into_owned(), s.style))
                            .collect();

                        if i == 0 {
                            let mut first =
                                vec![Span::raw(MARGIN), Span::styled("● ", marker_style)];
                            first.extend(owned_spans);
                            lines.push(Line::from(first));
                        } else {
                            let mut cont = vec![Span::raw(INDENT)];
                            cont.extend(owned_spans);
                            lines.push(Line::from(cont));
                        }
                    }
                    // Fallback if markdown produced nothing
                    if lines.is_empty() {
                        lines.push(Line::from(vec![
                            Span::raw(MARGIN),
                            Span::styled("● ", marker_style),
                        ]));
                    }
                }
            } else {
                lines.push(Line::from(vec![
                    Span::raw(MARGIN),
                    Span::styled("● ", marker_style),
                ]));
            }
        }
        EntryKind::ToolUse {
            tool,
            target,
            additions,
            deletions,
        } => {
            let mut spans = vec![
                Span::raw(MARGIN),
                Span::styled("● ", marker_style),
                Span::styled(tool.clone(), theme.tool_label),
                Span::raw(" "),
                Span::styled(target.clone(), theme.tool_target),
            ];
            if *additions > 0 {
                spans.push(Span::styled(format!(" +{additions}"), theme.diff_add));
            }
            if *deletions > 0 {
                spans.push(Span::styled(format!(" -{deletions}"), theme.diff_del));
            }
            lines.push(Line::from(spans));

            if !entry.body.is_empty() {
                for desc_line in entry.body.lines() {
                    lines.push(Line::from(vec![
                        Span::raw(INDENT),
                        Span::styled(desc_line.to_owned(), theme.dim_text),
                    ]));
                }
            }
        }
        EntryKind::Bash { command } => {
            lines.push(Line::from(vec![
                Span::raw(MARGIN),
                Span::styled("● ", marker_style),
                Span::styled("Bash", theme.tool_label),
                Span::styled(" (shell)", theme.dim_text),
            ]));

            lines.push(Line::from(vec![
                Span::raw(INDENT),
                Span::styled(command.clone(), theme.code_block),
            ]));
            for output_line in entry.body.lines() {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::raw("  "),
                    Span::styled(output_line.to_owned(), theme.dim_text),
                ]));
            }
        }
        EntryKind::Thinking => {
            let mut body_lines = entry.body.lines();
            let first = body_lines.next().unwrap_or("");
            lines.push(Line::from(vec![
                Span::raw(MARGIN),
                Span::styled("● ", marker_style),
                Span::styled(first.to_owned(), theme.dim_text),
            ]));
            for continuation in body_lines {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(continuation.to_owned(), theme.dim_text),
                ]));
            }
        }
        EntryKind::CodeBlock { language } => {
            if !language.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(language.clone(), theme.dim_text),
                ]));
            }
            for code_line in entry.body.lines() {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(code_line.to_owned(), theme.code_block),
                ]));
            }
        }
    }

    lines
}

fn marker_style_for(entry: &TimelineEntry, theme: &ChatTheme) -> Style {
    match (&entry.actor, &entry.kind) {
        (Actor::User, _) => theme.marker_response,
        (_, EntryKind::Thinking) => theme.marker_thinking,
        (_, EntryKind::ToolUse { .. } | EntryKind::Bash { .. }) => theme.marker_tool,
        (_, EntryKind::CodeBlock { .. }) => theme.marker_tool,
        _ => theme.marker_response,
    }
}

fn render_input(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    // Render prompt "›" on the left, TextArea takes the rest
    let [prompt_area, composer_area] = horizontal![==2, >=1].areas(area);

    // Prompt character
    let prompt =
        Paragraph::new(Line::from(Span::styled("› ", theme.input_prompt))).style(theme.timeline_bg);
    frame.render_widget(prompt, prompt_area);

    // Show placeholder if composer is empty
    if app.composer_text().is_empty() {
        let placeholder = Paragraph::new(Line::from(Span::styled(
            "Ask blazar…",
            theme.input_placeholder,
        )))
        .style(theme.timeline_bg);
        frame.render_widget(placeholder, composer_area);
    } else {
        let composer = app.composer();
        frame.render_widget(composer, composer_area);
    }
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let display_path = app.display_path();
    let branch = app.branch();
    let left = format!("blazar • {display_path} • {branch}");

    let status_label = app.status_label();
    let right_len = status_label.len();

    // Right-align the status label
    let available = area.width as usize;
    let gap = available.saturating_sub(left.len() + right_len);

    let line = Line::from(vec![
        Span::styled(left, theme.status_bar),
        Span::styled(" ".repeat(gap), theme.status_bar),
        Span::styled(status_label, theme.status_right),
    ]);

    let bar = Paragraph::new(line).style(theme.status_bar);
    frame.render_widget(bar, area);
}
