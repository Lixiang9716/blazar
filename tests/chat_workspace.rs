use blazar::chat::workspace::{WorkspaceApp, WorkspaceFocus, WorkspaceView};

#[test]
fn workspace_switches_views_and_cycles_focus() {
    let mut app = WorkspaceApp::new_for_test(env!("CARGO_MANIFEST_DIR"));

    assert_eq!(app.active_view(), WorkspaceView::Chat);
    assert_eq!(app.focus(), WorkspaceFocus::Nav);

    app.select_view(WorkspaceView::Git);
    assert_eq!(app.active_view(), WorkspaceView::Git);

    app.cycle_focus();
    assert_eq!(app.focus(), WorkspaceFocus::Content);

    app.cycle_focus();
    assert_eq!(app.focus(), WorkspaceFocus::Footer);
}
