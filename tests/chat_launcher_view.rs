use blazar::chat::launcher::LauncherApp;
use blazar::chat::launcher_view::{render_launcher, render_launcher_to_lines_for_test};
use blazar::chat::workspace_catalog::WorkspaceRecord;
use core::cmp;
use ratatui_core::{backend::TestBackend, terminal::Terminal};
use unicode_width::UnicodeWidthStr;

#[test]
fn wide_launcher_renders_workspace_list_preview_and_footer() {
    let app = LauncherApp::new(vec![
        WorkspaceRecord::named("blazar", "/home/lx/blazar"),
        WorkspaceRecord::named("graphify-lab", "/home/lx/graphify-lab"),
    ]);

    let lines = render_launcher_to_lines_for_test(&app, 110, 34);
    let all = lines.join("\n");

    assert!(all.contains("Workspace Launcher"));
    assert!(all.contains("Recent workspaces"));
    assert!(all.contains("Preview"));
    assert!(all.contains("Spirit"));
    assert!(all.contains("Enter resume"));
}

#[test]
fn narrow_launcher_renders_preview_and_footer() {
    let app = LauncherApp::new(vec![
        WorkspaceRecord::named("blazar", "/home/lx/blazar"),
        WorkspaceRecord::named("graphify-lab", "/home/lx/graphify-lab"),
    ]);

    let lines = render_launcher_to_lines_for_test(&app, 72, 28);
    let all = lines.join("\n");

    assert!(all.contains("Workspace Launcher"));
    assert!(all.contains("Preview"));
    assert!(all.contains("Spirit"));
    assert!(all.contains("Enter resume"));
    assert!(!all.contains("Recent workspaces"));
}

#[test]
fn empty_launcher_renders_empty_state_without_panicking() {
    let app = LauncherApp::new(vec![]);

    let lines = render_launcher_to_lines_for_test(&app, 110, 34);
    let all = lines.join("\n");

    assert!(all.contains("Workspace Launcher"));
    assert!(all.contains("No workspaces yet"));
    assert!(all.contains("Open Blazar from a repo"));
    assert!(all.contains("to populate this launcher"));
}

#[test]
fn launcher_test_helper_matches_buffer_extraction_for_wide_glyphs() {
    let app = LauncherApp::new(vec![WorkspaceRecord::named(
        "星糖-lab",
        "/home/lx/星糖-lab",
    )]);

    let actual = render_launcher_to_lines_for_test(&app, 110, 34);

    let backend = TestBackend::new(110, 34);
    let mut terminal = Terminal::new(backend).expect("launcher terminal should initialize");
    terminal
        .draw(|frame| render_launcher(frame, &app, 1_200))
        .expect("launcher frame should render");

    let expected = terminal
        .backend()
        .buffer()
        .content()
        .chunks(110)
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
        .collect::<Vec<_>>();

    assert_eq!(actual, expected);
}
