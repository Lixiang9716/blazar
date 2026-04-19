use blazar::chat::workspace::WorkspaceApp;
use blazar::chat::view::render_workspace_to_lines_for_test;
use insta::assert_snapshot;

#[test]
fn default_chat_frame_snapshot() {
    let app = WorkspaceApp::new_for_test(env!("CARGO_MANIFEST_DIR"));
    let lines = render_workspace_to_lines_for_test(&app, 60, 12);

    assert_snapshot!("default_chat_frame", lines.join("\n"));
}
