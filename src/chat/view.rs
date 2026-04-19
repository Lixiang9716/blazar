use crate::chat::app::ChatApp;
use crate::chat::model::{Actor, EntryKind, TimelineEntry};
use crate::chat::theme::{BASE03, ChatTheme, SolarizedStyleSheet, build_theme};
use crate::welcome::mascot::render_mascot_lines;
use crate::welcome::state::WelcomeState;
use core::cmp;
use ratatui_core::{
    backend::TestBackend,
    layout::{Constraint, Rect},
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
use tui_overlay::{Anchor, Backdrop, Overlay, Slide};
use tui_widget_list::{ListBuilder, ListView};
use unicode_width::UnicodeWidthStr;

pub fn render_to_lines_for_test(app: &mut ChatApp, width: u16, height: u16) -> Vec<String> {
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

pub fn render_frame(frame: &mut Frame, app: &mut ChatApp, tick_ms: u64) {
    let theme = build_theme();
    let area = frame.area();

    // Fill background
    let bg_block = Block::default().style(theme.timeline_bg);
    frame.render_widget(bg_block, area);

    // Vertical layout: welcome_banner | timeline | sep_top | input | sep_bot | status_bar
    let [banner, timeline, sep_top, input, sep_bot, status] =
        vertical![==12, >=1, ==1, ==1, ==1, ==1].areas(area);

    render_welcome_banner(frame, banner, app, tick_ms, &theme);
    render_timeline(frame, timeline, app, &theme);
    render_separator(frame, sep_top, &theme);
    render_input(frame, input, app, &theme);
    render_separator(frame, sep_bot, &theme);
    render_status_bar(frame, status, app, &theme);

    // Render modal picker overlay if visible
    if app.picker.is_visible() {
        render_picker(frame, area, app, &theme);
    }
}

fn render_welcome_banner(
    frame: &mut Frame,
    area: Rect,
    _app: &ChatApp,
    tick_ms: u64,
    theme: &ChatTheme,
) {
    let version = env!("CARGO_PKG_VERSION");

    // Animate mascot: use real elapsed time so sprite frames cycle
    let mascot_lines = render_mascot_lines(WelcomeState::new(), tick_ms);
    let mascot_rows: Vec<Line<'static>> = mascot_lines
        .into_iter()
        .skip_while(|line| {
            line.width() == 0 || line.spans.iter().all(|s| s.content.trim().is_empty())
        })
        .take_while(|line| {
            line.width() > 0 && !line.spans.iter().all(|s| s.content.trim().is_empty())
        })
        .collect();
    let mascot_width = mascot_rows.first().map(|l| l.width()).unwrap_or(0) as u16;

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.dim_text);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 2 || inner.height < 2 {
        return;
    }

    // Split inner: mascot | text
    let mascot_col_width = cmp::min(mascot_width + 1, inner.width / 3);
    let [mascot_area, text_area] = horizontal![==(mascot_col_width), >=1].areas(inner);

    // Render mascot sprite — vertically centered in its area
    let mascot_h = mascot_rows.len() as u16;
    let y_offset = mascot_area.height.saturating_sub(mascot_h) / 2;
    let centered_mascot = Rect::new(
        mascot_area.x,
        mascot_area.y + y_offset,
        mascot_area.width,
        cmp::min(mascot_h, mascot_area.height),
    );
    let mascot_paragraph = Paragraph::new(mascot_rows);
    frame.render_widget(mascot_paragraph, centered_mascot);

    // Render text beside mascot — vertically centered
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
        Line::from(vec![
            Span::styled("Tip: ", theme.dim_text),
            Span::styled("/help", theme.tip_command),
            Span::styled(
                " for commands. Blazar uses AI. Check for mistakes.",
                theme.dim_text,
            ),
        ]),
    ];
    let text_h = text_lines.len() as u16;
    let text_y_offset = text_area.height.saturating_sub(text_h) / 2;
    let centered_text = Rect::new(
        text_area.x,
        text_area.y + text_y_offset,
        text_area.width,
        cmp::min(text_h, text_area.height),
    );
    let text_paragraph = Paragraph::new(text_lines);
    frame.render_widget(text_paragraph, centered_text);
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

    // Feed back heights so scroll sentinel can be resolved
    app.timeline_content_height.set(content_height);
    app.timeline_visible_height.set(visible_height);

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
                    // Assistant messages: render markdown with Solarized theme
                    let opts = tui_markdown::Options::new(SolarizedStyleSheet);
                    let md_text = tui_markdown::from_str_with_options(&entry.body, &opts);
                    for (i, md_line) in md_text.lines.into_iter().enumerate() {
                        let owned_spans: Vec<Span<'a>> = md_line
                            .spans
                            .into_iter()
                            .map(|s| {
                                // Strip heading `# ` prefixes — tui-markdown keeps them raw
                                let content = s.content.into_owned();
                                let trimmed = content.trim_start_matches('#');
                                if trimmed.len() < content.len() {
                                    Span::styled(trimmed.trim_start().to_owned(), s.style)
                                } else {
                                    Span::styled(content, s.style)
                                }
                            })
                            .filter(|s| !s.content.is_empty())
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
        EntryKind::Hint => {
            let mut body_lines = entry.body.lines();
            let first = body_lines.next().unwrap_or("");
            lines.push(Line::from(vec![
                Span::raw(MARGIN),
                Span::styled("● ", marker_style),
                Span::styled("💡 ", marker_style),
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
        (_, EntryKind::Hint) => theme.marker_hint,
        (_, EntryKind::Thinking) => theme.marker_thinking,
        (_, EntryKind::ToolUse { .. } | EntryKind::Bash { .. }) => theme.marker_tool,
        (_, EntryKind::CodeBlock { .. }) => theme.marker_tool,
        _ => theme.marker_response,
    }
}

fn render_input(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &ChatTheme) {
    let [prompt_area, composer_area] = horizontal![==2, >=1].areas(area);

    let prompt =
        Paragraph::new(Line::from(Span::styled("› ", theme.input_prompt))).style(theme.timeline_bg);
    frame.render_widget(prompt, prompt_area);

    if app.composer_text().is_empty() {
        // Empty — show nothing, just the cursor position
        let empty = Paragraph::new("").style(theme.timeline_bg);
        frame.render_widget(empty, composer_area);
    } else {
        let composer = app.composer();
        frame.render_widget(composer, composer_area);
    }
}

fn render_separator(frame: &mut Frame, area: Rect, theme: &ChatTheme) {
    let line = Line::from(Span::styled(
        "─".repeat(area.width as usize),
        theme.dim_text,
    ));
    let bar = Paragraph::new(line);
    frame.render_widget(bar, area);
}

fn render_status_bar(frame: &mut Frame, area: Rect, _app: &ChatApp, theme: &ChatTheme) {
    let left = "/ commands · ? help";
    let right = "blazar-dev (local)";

    let available = area.width as usize;
    let gap = available.saturating_sub(left.len() + right.len());

    let line = Line::from(vec![
        Span::styled(left, theme.status_bar),
        Span::styled(" ".repeat(gap), theme.status_bar),
        Span::styled(right, theme.status_right),
    ]);

    let bar = Paragraph::new(line).style(theme.status_bar);
    frame.render_widget(bar, area);
}

fn render_picker(frame: &mut Frame, full_area: Rect, app: &mut ChatApp, theme: &ChatTheme) {
    use crate::chat::picker::PICKER_PAGE_SIZE;

    let filtered_items: Vec<(String, String)> = app
        .picker
        .filtered_items()
        .into_iter()
        .map(|item| (item.label.clone(), item.description.clone()))
        .collect();
    let total = filtered_items.len();
    if full_area.width < 8 || full_area.height < 6 {
        return;
    }

    let visible_count = total.min(PICKER_PAGE_SIZE) as u16;
    // title(1) + visible items + footer(1) + border(2)
    let picker_h = cmp::min(visible_count + 4, full_area.height.saturating_sub(2));
    let picker_w = cmp::min(50, full_area.width.saturating_sub(4));

    let overlay = Overlay::new()
        .anchor(Anchor::BottomLeft)
        .offset(2, -2)
        .width(Constraint::Length(picker_w))
        .height(Constraint::Length(picker_h))
        .slide(Slide::Bottom)
        .backdrop(Backdrop::new(BASE03))
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme.picker_title)
                .title(Line::from(Span::styled(
                    format!(" {} ", app.picker.title),
                    theme.picker_title,
                ))),
        );
    frame.render_stateful_widget(overlay, full_area, app.picker.overlay_state_mut());

    let Some(inner) = app.picker.overlay_state().inner_area() else {
        return;
    };
    if inner.height < 2 || inner.width < 4 || picker_h == 0 || picker_w == 0 {
        return;
    }

    // Reserve last row for footer
    let items_h = inner.height.saturating_sub(1);
    let [items_area, footer_area] = vertical![>=(items_h), ==1].areas(inner);

    let has_up = app.picker.has_scroll_up();
    let has_down = app.picker.has_scroll_down();
    let top_hint = u16::from(has_up);
    let bottom_hint = u16::from(has_down);
    if items_area.height <= top_hint + bottom_hint {
        return;
    }
    let [top_hint_area, list_area, bottom_hint_area] =
        vertical![==(top_hint), >=1, ==(bottom_hint)].areas(items_area);

    if has_up {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("  ▲ more", theme.dim_text))),
            top_hint_area,
        );
    }

    if total == 0 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  No matching commands",
                theme.dim_text,
            ))),
            list_area,
        );
    } else {
        let list_builder = ListBuilder::new(|context| {
            let item = &filtered_items[context.index];
            let marker = if context.is_selected { "› " } else { "  " };
            let label_style = if context.is_selected {
                theme.picker_selected
            } else {
                theme.picker_item
            };

            let mut spans = vec![
                Span::styled(marker, label_style),
                Span::styled(item.0.clone(), label_style),
            ];
            if !item.1.is_empty() {
                spans.push(Span::styled(format!("  {}", item.1), theme.picker_desc));
            }

            (Line::from(spans), 1)
        });
        let list = ListView::new(list_builder, total).scroll_padding(1);
        frame.render_stateful_widget(list, list_area, app.picker.list_state_mut());
    }

    if has_down {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("  ▼ more", theme.dim_text))),
            bottom_hint_area,
        );
    }

    // Footer with count info
    let footer_text = format!(
        "↑↓ navigate · enter select · esc cancel  ({}/{})",
        app.picker.selected_index().map_or(0, |index| index + 1),
        total
    );
    let footer = Line::from(Span::styled(footer_text, theme.dim_text));
    frame.render_widget(Paragraph::new(footer), footer_area);
}
