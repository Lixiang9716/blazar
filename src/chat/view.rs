use crate::chat::app::ChatApp;
use crate::chat::git::GitSummary;
use crate::chat::model::Author;
use crate::chat::session::SessionSummary;
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

pub fn render_workspace_to_lines_for_test(
    app: &WorkspaceApp,
    width: u16,
    height: u16,
) -> Vec<String> {
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
    let area = frame.area();
    if area.width < 80 {
        render_workspace_narrow(frame, app, area);
    } else {
        render_workspace_wide(frame, app, tick_ms, area);
    }
}

fn render_workspace_narrow(frame: &mut Frame, app: &WorkspaceApp, area: Rect) {
    let theme = build_theme();

    // Compact: nav bar (1 line) + content (fill) + footer (3 lines)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(area);

    // Compact nav bar
    let nav_line = Line::from(Span::styled("Chat · Git · Sessions", theme.status_text));
    frame.render_widget(Paragraph::new(nav_line), rows[0]);

    // Content panel
    match app.active_view() {
        WorkspaceView::Chat => render_messages_only(frame, rows[1], app.chat(), &theme),
        WorkspaceView::Git => render_git_panel(frame, rows[1], app.git_summary(), &theme),
        WorkspaceView::Sessions => {
            render_session_panel(frame, rows[1], app.session_summary(), &theme)
        }
    }

    // Footer
    render_footer(frame, rows[2], app, &theme);
}

fn render_workspace_wide(frame: &mut Frame, app: &WorkspaceApp, tick_ms: u64, area: Rect) {
    let theme = build_theme();

    // Header, body, footer
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
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
    let views = [
        WorkspaceView::Chat,
        WorkspaceView::Git,
        WorkspaceView::Sessions,
    ];
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
            render_git_panel(frame, cols[1], app.git_summary(), &theme);
        }
        WorkspaceView::Sessions => {
            render_session_panel(frame, cols[1], app.session_summary(), &theme);
        }
    }

    // Footer
    render_footer(frame, rows[2], app, &theme);
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

fn render_git_panel(
    frame: &mut Frame,
    area: Rect,
    summary: &GitSummary,
    theme: &crate::chat::theme::ChatTheme,
) {
    let mut lines: Vec<Line> = vec![];

    let status_label = if summary.is_dirty { "dirty" } else { "clean" };
    lines.push(Line::from(vec![
        Span::styled("Branch: ", Style::default()),
        Span::styled(&summary.branch, theme.status_text),
        Span::styled("  ", Style::default()),
        Span::styled(
            status_label,
            if summary.is_dirty {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Green)
            },
        ),
    ]));

    lines.push(Line::from(format!(
        "ahead: {}  behind: {}  staged: {}  unstaged: {}",
        summary.ahead, summary.behind, summary.staged, summary.unstaged
    )));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Changed files:",
        Style::default().fg(Color::Cyan),
    )));
    if summary.changed_files.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Working tree clean",
            Style::default().fg(Color::Green),
        )));
    } else {
        for f in &summary.changed_files {
            lines.push(Line::from(format!("  {f}")));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Recent commits:",
        Style::default().fg(Color::Cyan),
    )));
    if summary.recent_commits.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No recent commits",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for c in &summary.recent_commits {
            lines.push(Line::from(format!("  {c}")));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.panel_border)
        .title("Git");

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn render_footer(
    frame: &mut Frame,
    area: Rect,
    app: &WorkspaceApp,
    theme: &crate::chat::theme::ChatTheme,
) {
    if app.active_view() == WorkspaceView::Chat {
        let footer_block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.panel_border)
            .title("Ask Spirit");
        let footer_inner = footer_block.inner(area);
        frame.render_widget(footer_block, area);
        let composer = app.chat().composer();
        frame.render_widget(composer, footer_inner);
    } else {
        let hint_text = Line::from(Span::styled(
            "[1] Chat  [2] Git  [3] Sessions  [Tab] Focus  [Esc] Quit",
            theme.status_text,
        ));
        let footer_block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.panel_border)
            .title("Workspace Hints");
        let footer_inner = footer_block.inner(area);
        frame.render_widget(footer_block, area);
        frame.render_widget(Paragraph::new(hint_text), footer_inner);
    }
}

fn render_session_panel(
    frame: &mut Frame,
    area: Rect,
    summary: &SessionSummary,
    theme: &crate::chat::theme::ChatTheme,
) {
    let mut lines: Vec<Line> = vec![];

    if summary.session_label.is_empty() {
        lines.push(Line::from("No session details available yet"));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Session: ", Style::default()),
            Span::styled(&summary.session_label, theme.status_text),
        ]));

        if !summary.cwd.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Repo: ", Style::default()),
                Span::styled(&summary.cwd, Style::default().fg(Color::Cyan)),
            ]));
        }

        if !summary.active_intent.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Intent: ", Style::default()),
                Span::styled(&summary.active_intent, Style::default().fg(Color::Magenta)),
            ]));
        }

        if !summary.plan_status.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Plan: ", Style::default()),
                Span::styled(&summary.plan_status, Style::default().fg(Color::Green)),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Checkpoints:",
            Style::default().fg(Color::Cyan),
        )));
        if summary.checkpoints.is_empty() {
            lines.push(Line::from("  No checkpoints recorded"));
        } else {
            for cp in &summary.checkpoints {
                lines.push(Line::from(format!("  {cp}")));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Todos:",
            Style::default().fg(Color::Cyan),
        )));
        lines.push(Line::from(format!(
            "  done: {}  in progress: {}  ready: {}",
            summary.done_todos, summary.in_progress_todos, summary.ready_todos
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.panel_border)
        .title("Sessions");

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}
