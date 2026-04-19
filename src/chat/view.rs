use crate::chat::app::ChatApp;
use crate::chat::model::Author;
use crate::chat::theme::build_theme;
use crate::chat::workspace::{WorkspaceApp, WorkspaceView};
use crate::welcome::mascot::render_mascot_lines;
use crate::welcome::state::WelcomeState;
use core::cmp;
use ratatui_core::{
    backend::TestBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    terminal::{Frame, Terminal},
    text::{Line, Span},
};
use ratatui_widgets::{
    block::Block,
    borders::Borders,
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

pub fn render_workspace_to_lines_for_test(app: &WorkspaceApp, width: u16, height: u16) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

    terminal
        .draw(|frame| render_workspace(frame, app, 1_200))
        .expect("workspace frame should render");

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

pub fn render_frame(frame: &mut Frame, app: &ChatApp, tick_ms: u64) {
    let theme = build_theme();
    let area = frame.area();

    // Split into left Spirit pane and right chat pane
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    render_spirit_pane(frame, chunks[0], tick_ms, &theme);
    render_chat_pane(frame, chunks[1], app, &theme);
}

fn render_spirit_pane(
    frame: &mut Frame,
    area: Rect,
    tick_ms: u64,
    theme: &crate::chat::theme::ChatTheme,
) {
    let state = WelcomeState::new().tick(tick_ms, false);
    let mascot_lines = render_mascot_lines(state, tick_ms);

    let title_line = Line::from(vec![
        Span::styled("Spirit / ", Style::default().fg(Color::Cyan)),
        Span::styled("星糖导航马", Style::default().fg(Color::Magenta)),
    ]);

    let status = "Waiting with a sprinkle of stardust";

    let mut text_lines = vec![title_line, Line::from(status), Line::from("")];
    text_lines.extend(mascot_lines);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.spirit_border)
        .title("Spirit");

    let paragraph = Paragraph::new(text_lines)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

pub fn render_workspace(frame: &mut Frame, app: &WorkspaceApp, tick_ms: u64) {
    let theme = build_theme();
    let area = frame.area();

    // Header, body, footer
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    // Header
    let header = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(theme.shell_border)
        .title("Blazar · Spirit Workspace");
    let header_para = Paragraph::new(Line::from(""))
        .block(header)
        .wrap(Wrap { trim: false });
    frame.render_widget(header_para, rows[0]);

    // Body: rail (left) + content (right)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(10)])
        .split(rows[1]);

    // Rail
    let mut rail_lines: Vec<Line> = vec![];
    let state = WelcomeState::new().tick(tick_ms, false);
    let mut mascot_lines = render_mascot_lines(state, tick_ms);
    // keep mascot compact
    mascot_lines.truncate(3);

    rail_lines.push(Line::from(Span::styled("Spirit", theme.status_text)));
    rail_lines.push(Line::from(""));
    for line in mascot_lines {
        rail_lines.push(line);
    }
    rail_lines.push(Line::from(""));

    // Navigation items
    let views = [WorkspaceView::Chat, WorkspaceView::Git, WorkspaceView::Sessions];
    for v in views.iter() {
        let label = match v {
            WorkspaceView::Chat => "Chat",
            WorkspaceView::Git => "Git",
            WorkspaceView::Sessions => "Sessions",
        };
        let style = if *v == app.active_view() {
            theme.active_nav
        } else {
            theme.inactive_nav
        };
        rail_lines.push(Line::from(Span::styled(label, style)));
    }

    let rail_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.rail_border)
        .title("Nav");
    let rail_para = Paragraph::new(rail_lines).block(rail_block);
    frame.render_widget(rail_para, cols[0]);

    // Content
    match app.active_view() {
        WorkspaceView::Chat => {
            // render messages only in the main content area; the real composer belongs in the footer
            render_messages_only(frame, cols[1], app.chat(), &theme);
        }
        WorkspaceView::Git => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(theme.panel_border)
                .title("Git");
            let paragraph = Paragraph::new(Line::from("View not implemented yet")).block(block);
            frame.render_widget(paragraph, cols[1]);
        }
        WorkspaceView::Sessions => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(theme.panel_border)
                .title("Sessions");
            let paragraph = Paragraph::new(Line::from("View not implemented yet")).block(block);
            frame.render_widget(paragraph, cols[1]);
        }
    }

    // Footer / composer title - render the block, then render the composer into the block's inner area
    let footer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.panel_border)
        .title("Ask Spirit");
    // compute inner rect using Block::inner before moving the block into render_widget
    let footer_inner = footer_block.inner(rows[2]);
    frame.render_widget(footer_block, rows[2]);
    // render the actual composer TextArea from the chat app into the footer inner area
    let composer = app.chat().composer();
    frame.render_widget(composer, footer_inner);
}

fn render_chat_pane(
    frame: &mut Frame,
    area: Rect,
    app: &ChatApp,
    theme: &crate::chat::theme::ChatTheme,
) {
    // Split chat area into messages (top) and composer (bottom)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    render_messages(frame, chunks[0], app, theme);
    render_composer(frame, chunks[1], app, theme);
}

// messages-only renderer (no composer) for workspace content area
fn render_messages_only(
    frame: &mut Frame,
    area: Rect,
    app: &ChatApp,
    theme: &crate::chat::theme::ChatTheme,
) {
    let mut text_lines = vec![];

    for msg in app.messages() {
        let style = match msg.author {
            Author::Spirit => theme.spirit_bubble,
            Author::User => theme.user_bubble,
        };
        text_lines.push(Line::from(Span::styled(&msg.body, style)));
        text_lines.push(Line::from(""));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.chat_border)
        .title("Chat");

    let paragraph = Paragraph::new(text_lines)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn render_messages(
    frame: &mut Frame,
    area: Rect,
    app: &ChatApp,
    theme: &crate::chat::theme::ChatTheme,
) {
    render_messages_only(frame, area, app, theme);
}

fn render_composer(
    frame: &mut Frame,
    area: Rect,
    app: &ChatApp,
    _theme: &crate::chat::theme::ChatTheme,
) {
    // TextArea widget requires immutable reference
    let composer = app.composer();
    frame.render_widget(composer, area);
}
