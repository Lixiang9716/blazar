use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    text::Line,
};
use serde_json::json;

use crate::{
    tui::{
        app::status::READY_STATUS,
        model::form_schema_from_ui_ast,
        state::{FormState, LayoutNavModel},
        view::{HelpOverlayPage, HelpOverlayRender, HelpShortcutRender, UiContext, draw},
    },
    ui_ast::{build_ui_ast, layout::build_ui_layout},
};

#[test]
fn draw_renders_header_lines_above_body_and_footer() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": {
                "title": "Name",
                "type": "string"
            }
        }
    });

    let ast = build_ui_ast(&schema).expect("ast");
    let form_schema = form_schema_from_ui_ast(&ast);
    let layout = build_ui_layout(&ast);
    let mut form_state = FormState::from_schema(&form_schema);
    form_state.set_layout_nav(LayoutNavModel::from_uilayout(&layout));
    assert!(form_state.focus_first_field_with_layout());

    let header_lines = vec![Line::from("SchemaUI mascot")];
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            draw(
                frame,
                &mut form_state,
                None,
                UiContext {
                    status_message: READY_STATUS,
                    dirty: false,
                    error_count: 0,
                    help: Some("Ctrl+S -> validate and save"),
                    global_errors: &[],
                    focus_label: None,
                    session_title: Some("SchemaUI Demo"),
                    header_lines: Some(header_lines.as_slice()),
                    popup: None,
                    composite_overlay: None,
                    help_overlay: None,
                },
            );
        })
        .expect("frame render succeeds");

    let rendered = buffer_lines(terminal.backend().buffer(), 80, 20);
    let header_row = find_line_index(&rendered, "SchemaUI mascot").expect("header row");
    let body_row = find_line_index(&rendered, "Name").expect("body row");
    let footer_row = find_line_index(&rendered, "Ready for input").expect("footer row");

    assert_eq!(header_row, 0, "expected header at the top: {rendered:#?}");
    assert!(
        header_row < body_row,
        "expected header above the form body: {rendered:#?}"
    );
    assert!(
        body_row < footer_row,
        "expected footer below the form body: {rendered:#?}"
    );
    assert!(
        rendered.iter().any(|line| line.contains("SchemaUI mascot")),
        "expected header text to appear in the rendered frame: {rendered:#?}"
    );
    assert!(
        rendered.iter().any(|line| line.contains("Name")),
        "expected form body to remain visible: {rendered:#?}"
    );
    assert!(
        rendered.iter().any(|line| line.contains("Ready for input")),
        "expected footer status to remain visible: {rendered:#?}"
    );
}

#[test]
fn draw_without_header_keeps_body_and_footer_visible() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": {
                "title": "Name",
                "type": "string"
            }
        }
    });

    let ast = build_ui_ast(&schema).expect("ast");
    let form_schema = form_schema_from_ui_ast(&ast);
    let layout = build_ui_layout(&ast);
    let mut form_state = FormState::from_schema(&form_schema);
    form_state.set_layout_nav(LayoutNavModel::from_uilayout(&layout));
    assert!(form_state.focus_first_field_with_layout());

    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            draw(
                frame,
                &mut form_state,
                None,
                UiContext {
                    status_message: READY_STATUS,
                    dirty: false,
                    error_count: 0,
                    help: Some("Ctrl+S -> validate and save"),
                    global_errors: &[],
                    focus_label: None,
                    session_title: Some("SchemaUI Demo"),
                    header_lines: None,
                    popup: None,
                    composite_overlay: None,
                    help_overlay: None,
                },
            );
        })
        .expect("frame render succeeds");

    let rendered = buffer_lines(terminal.backend().buffer(), 80, 20);
    assert!(
        rendered.iter().any(|line| line.contains("Name")),
        "expected form body to remain visible: {rendered:#?}"
    );
    assert!(
        rendered.iter().any(|line| line.contains("Ready for input")),
        "expected footer status to remain visible: {rendered:#?}"
    );
    assert!(
        rendered.iter().all(|line| !line.contains("SchemaUI mascot")),
        "expected header text to be absent without header lines: {rendered:#?}"
    );
}

#[test]
fn draw_keeps_header_visible_when_help_overlay_is_open() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": {
                "title": "Name",
                "type": "string"
            }
        }
    });

    let ast = build_ui_ast(&schema).expect("ast");
    let form_schema = form_schema_from_ui_ast(&ast);
    let layout = build_ui_layout(&ast);
    let mut form_state = FormState::from_schema(&form_schema);
    form_state.set_layout_nav(LayoutNavModel::from_uilayout(&layout));
    assert!(form_state.focus_first_field_with_layout());

    let header_lines = vec![Line::from("SchemaUI mascot")];
    let help_page = HelpOverlayPage {
        summary: "Help summary".to_string(),
        current_page: 1,
        total_pages: 1,
        shortcuts: vec![HelpShortcutRender {
            scope: "Form".to_string(),
            keys: "Ctrl+S".to_string(),
            action: "validate and save".to_string(),
        }],
        errors: Vec::new(),
        total_errors: 0,
    };
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            draw(
                frame,
                &mut form_state,
                None,
                UiContext {
                    status_message: READY_STATUS,
                    dirty: false,
                    error_count: 0,
                    help: Some("Ctrl+S -> validate and save"),
                    global_errors: &[],
                    focus_label: None,
                    session_title: Some("SchemaUI Demo"),
                    header_lines: Some(header_lines.as_slice()),
                    popup: None,
                    composite_overlay: None,
                    help_overlay: Some(HelpOverlayRender {
                        page: &help_page,
                        shortcut_offset: 0,
                        error_offset: 0,
                    }),
                },
            );
        })
        .expect("frame render succeeds");

    let rendered = buffer_lines(terminal.backend().buffer(), 80, 20);
    assert!(
        rendered
            .first()
            .is_some_and(|line| line.contains("SchemaUI mascot")),
        "expected header to remain visible at the top while help is open: {rendered:#?}"
    );
}

fn buffer_lines(buffer: &Buffer, width: u16, height: u16) -> Vec<String> {
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

fn find_line_index(lines: &[String], needle: &str) -> Option<usize> {
    lines.iter().position(|line| line.contains(needle))
}
