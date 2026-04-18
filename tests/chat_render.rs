use blazar::chat::app::ChatApp;
use blazar::chat::view::render_to_lines_for_test;

#[test]
fn chat_view_renders_spirit_pane_and_message_pane() {
    let app = ChatApp::new_for_test("/home/lx/blazar");
    let lines = render_to_lines_for_test(&app, 100, 30);

    assert!(lines.iter().any(|line| line.contains("Spirit / 星糖导航马")));
    assert!(lines.iter().any(|line| line.contains("Tell me what you'd like to explore")));
}
