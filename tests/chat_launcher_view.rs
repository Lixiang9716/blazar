use blazar::chat::launcher::LauncherApp;
use blazar::chat::launcher_view::render_launcher_to_lines_for_test;
use blazar::chat::workspace_catalog::WorkspaceRecord;

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
