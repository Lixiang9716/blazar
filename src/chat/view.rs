use crate::chat::app::ChatApp;
use crate::chat::model::Author;
use crate::chat::theme::build_theme;
use crate::welcome::mascot::{render_mascot_lines, render_mascot_plain};
use crate::welcome::state::WelcomeState;
use ratatui_core::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    terminal::Frame,
};
use ratatui_widgets::{
    block::Block,
    borders::Borders,
    paragraph::{Paragraph, Wrap},
};

pub fn render_to_lines_for_test(app: &ChatApp, width: u16, height: u16) -> Vec<String> {
    let _ = (width, height);
    
    // Render mascot in idle state
    let tick_ms = 1_200;
    let mascot = render_mascot_plain(WelcomeState::new().tick(tick_ms, false), tick_ms);
    
    let mut lines = vec![
        "Spirit / 星糖导航马".to_owned(),
        "Waiting with a sprinkle of stardust".to_owned(),
    ];
    
    // Add mascot lines
    for line in mascot.lines() {
        lines.push(line.to_owned());
    }
    
    // Add message
    lines.push(app.messages()[0].body.clone());
    
    let composer_content = app.composer_text();
    if !composer_content.is_empty() {
        lines.push(format!("Composer: {}", composer_content));
    }
    
    lines
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

fn render_spirit_pane(frame: &mut Frame, area: Rect, tick_ms: u64, theme: &crate::chat::theme::ChatTheme) {
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

fn render_chat_pane(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &crate::chat::theme::ChatTheme) {
    // Split chat area into messages (top) and composer (bottom)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    render_messages(frame, chunks[0], app, theme);
    render_composer(frame, chunks[1], app, theme);
}

fn render_messages(frame: &mut Frame, area: Rect, app: &ChatApp, theme: &crate::chat::theme::ChatTheme) {
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

fn render_composer(frame: &mut Frame, area: Rect, app: &ChatApp, _theme: &crate::chat::theme::ChatTheme) {
    // TextArea widget requires immutable reference
    let composer = app.composer();
    frame.render_widget(composer, area);
}
