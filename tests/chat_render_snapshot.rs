use blazar::chat::app::ChatApp;
use blazar::chat::view::render_to_lines_for_test;
use insta::assert_snapshot;

#[test]
fn default_chat_frame_snapshot() {
    let app = ChatApp::new_for_test(env!("CARGO_MANIFEST_DIR"));
    let lines = render_to_lines_for_test(&app, 60, 12);

    assert_snapshot!("default_chat_frame", lines.join("\n"));
}
