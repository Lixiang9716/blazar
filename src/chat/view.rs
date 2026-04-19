use crate::chat::app::ChatApp;
use crate::chat::model::{Actor, EntryKind, TimelineEntry};
use crate::chat::theme::{ChatTheme, build_theme};
use crate::welcome::mascot::render_mascot_lines;
use crate::welcome::state::WelcomeState;
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
    borders::BorderType,
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

    // Vertical layout: welcome_banner | timeline | separator | input | status_bar
    let [banner, timeline, sep, input, status] = vertical![==6, >=1, ==1, ==3, ==1].areas(area);

    render_welcome_banner(frame, banner, &theme);
    render_timeline(frame, timeline, app, &theme);
    render_separator(frame, sep, &theme);
    render_input(frame, input, app, &theme);
    render_status_bar(frame, status, app, &theme);
}

fn render_welcome_banner(frame: &mut Frame, area: Rect, theme: &ChatTheme) {
    let version = env!("CARGO_PKG_VERSION");

    // Get mascot sprite lines (first idle frame) — take top 4 rows as icon
    let mascot_lines = render_mascot_lines(WelcomeState::new(), 0);
    let mascot_rows: Vec<Line<'static>> = mascot_lines.into_iter().take(4).collect();
    let mascot_width = mascot_rows.first().map(|l| l.width()).unwrap_or(0) as u16;

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.dim_text);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 2 || inner.height < 4 {
        return;
    }

    // Split inner: mascot | text
    let mascot_col_width = cmp::min(mascot_width + 1, inner.width / 3);
    let [mascot_area, text_area] = horizontal![==(mascot_col_width), >=1].areas(inner);

    // Render mascot sprite
    let mascot_paragraph = Paragraph::new(mascot_rows);
    frame.render_widget(mascot_paragraph, mascot_area);

    // Render text beside mascot
    let text_lines = vec![
        Line::from(vec![
            Span::styled("Blazar", theme.title_text),
            Span::styled(format!(" v{version}"), theme.dim_text),
        ]),
        Line::from(Span::styled(
            "Describe a task to get started.",
            theme.body_text,
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Tip: /help for commands. Blazar uses AI. Check for mistakes.",
            theme.dim_text,
        )),
    ];
    let text_paragraph = Paragraph::new(text_lines);
    frame.render_widget(text_paragraph, text_area);
}

fn render_timeline(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let mut lines: Vec<Line> = Vec::new();
    let show_details = app.show_details();

    for entry in app.timeline() {
        let entry_lines = render_entry(entry, theme, area.width);
        lines.extend(entry_lines);

        // Show expanded details when Ctrl+O is toggled
        if show_details && !entry.details.is_empty() {
            lines.push(Line::from(""));
            for detail_line in entry.details.lines() {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(detail_line.to_owned(), theme.dim_text),
                ]));
            }
        }

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
                    // User messages: `›` prefix, plain bold text (no markdown)
                    let mut body_lines = entry.body.lines();
                    if let Some(first) = body_lines.next() {
                        lines.push(Line::from(vec![
                            Span::raw(MARGIN),
                            Span::styled("› ", marker_style),
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
        EntryKind::Warning => {
            let mut body_lines = entry.body.lines();
            let first = body_lines.next().unwrap_or("");
            lines.push(Line::from(vec![
                Span::raw(MARGIN),
                Span::styled("! ", marker_style),
                Span::styled(first.to_owned(), theme.body_text),
            ]));
            for continuation in body_lines {
                lines.push(Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(continuation.to_owned(), theme.body_text),
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
        (_, EntryKind::Warning) => theme.marker_warning,
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
            "Type @ to mention files, # for issues/PRs, / for commands, or ? for shortcuts",
            theme.input_placeholder,
        )))
        .style(theme.timeline_bg);
        frame.render_widget(placeholder, composer_area);
    } else {
        let composer = app.composer();
        frame.render_widget(composer, composer_area);
    }
}

fn render_separator(frame: &mut Frame, area: Rect, theme: &ChatTheme) {
    let model_label = "blazar-dev";
    let model_len = model_label.len();
    let line_len = (area.width as usize).saturating_sub(model_len + 1);

    let line = Line::from(vec![
        Span::styled("─".repeat(line_len), theme.dim_text),
        Span::raw(" "),
        Span::styled(model_label, theme.status_right),
    ]);
    let bar = Paragraph::new(line);
    frame.render_widget(bar, area);
}

fn render_status_bar(frame: &mut Frame, area: Rect, _app: &ChatApp, theme: &ChatTheme) {
    let left = "shift+tab switch mode";
    let right = "ready";
    let right_len = right.len();

    // Right-align the status label
    let available = area.width as usize;
    let gap = available.saturating_sub(left.len() + right_len);

    let line = Line::from(vec![
        Span::styled(left, theme.status_bar),
        Span::styled(" ".repeat(gap), theme.status_bar),
        Span::styled(right, theme.status_right),
    ]);

    let bar = Paragraph::new(line).style(theme.status_bar);
    frame.render_widget(bar, area);
}
