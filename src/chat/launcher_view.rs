use crate::chat::launcher::LauncherApp;
use crate::welcome::mascot::render_mascot_lines;
use crate::welcome::state::WelcomeState;
use core::cmp;
use ratatui_core::{
    backend::TestBackend,
    layout::{Constraint, Direction, Layout, Rect},
    terminal::{Frame, Terminal},
    text::Line,
};
use ratatui_widgets::{block::Block, borders::Borders, paragraph::Paragraph};
use unicode_width::UnicodeWidthStr;

pub fn render_launcher_to_lines_for_test(
    app: &LauncherApp,
    width: u16,
    height: u16,
) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("launcher terminal should initialize");
    terminal
        .draw(|frame| render_launcher(frame, app, 1_200))
        .expect("launcher frame should render");
    terminal
        .backend()
        .buffer()
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

pub fn render_launcher(frame: &mut Frame, app: &LauncherApp, tick_ms: u64) {
    if frame.area().width < 80 {
        render_launcher_narrow(frame, app, tick_ms);
    } else {
        render_launcher_wide(frame, app, tick_ms);
    }
}

fn render_launcher_wide(frame: &mut Frame, app: &LauncherApp, tick_ms: u64) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.area());
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(rows[1]);
    render_launcher_header(frame, rows[0]);
    render_workspace_list(frame, cols[0], app);
    render_workspace_preview(frame, cols[1], app, tick_ms);
    render_launcher_footer(frame, rows[2]);
}

fn render_launcher_narrow(frame: &mut Frame, app: &LauncherApp, tick_ms: u64) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(frame.area());
    render_launcher_header(frame, rows[0]);
    render_workspace_preview(frame, rows[1], app, tick_ms);
    render_launcher_footer(frame, rows[2]);
}

fn render_launcher_header(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Blazar · Workspace Launcher");
    frame.render_widget(block, area);
}

fn render_workspace_list(frame: &mut Frame, area: Rect, app: &LauncherApp) {
    if !app.has_workspaces() {
        frame.render_widget(
            Paragraph::new("No recent workspaces").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Recent workspaces"),
            ),
            area,
        );
        return;
    }

    let items = app
        .workspaces()
        .iter()
        .enumerate()
        .map(|(idx, ws)| {
            if idx == app.selected_index() {
                Line::from(format!("> {} · {}", ws.name, ws.repo_path))
            } else {
                Line::from(format!("  {} · {}", ws.name, ws.repo_path))
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Recent workspaces"),
        ),
        area,
    );
}

fn render_workspace_preview(frame: &mut Frame, area: Rect, app: &LauncherApp, tick_ms: u64) {
    if !app.has_workspaces() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from("No workspaces yet"),
                Line::from(""),
                Line::from("Open Blazar from a repo"),
                Line::from("to populate this launcher"),
            ])
            .block(Block::default().borders(Borders::ALL).title("Preview")),
            area,
        );
        return;
    }

    let selected = app.selected_workspace();
    let mut mascot_lines = render_mascot_lines(WelcomeState::new().tick(tick_ms, false), tick_ms);
    mascot_lines.truncate(2);
    let lines = vec![
        Line::from(format!("Workspace: {}", selected.name)),
        Line::from(format!("Branch: {}", selected.branch)),
        Line::from(format!("Ready todos: {}", selected.ready_todos)),
        Line::from(""),
        Line::from("Spirit"),
    ];
    let block = Block::default().borders(Borders::ALL).title("Preview");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(lines.len() as u16), Constraint::Min(2)])
        .split(inner);
    frame.render_widget(Paragraph::new(lines), layout[0]);
    frame.render_widget(Paragraph::new(mascot_lines), layout[1]);
}

fn render_launcher_footer(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new("Enter resume · S sessions · G git · Tab focus · Esc quit")
            .block(Block::default().borders(Borders::ALL)),
        area,
    );
}
