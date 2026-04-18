use blazar::chat::app::ChatApp;

#[test]
fn chat_app_starts_with_a_spirit_greeting_message() {
    let app = ChatApp::new_for_test("/home/lx/blazar");

    assert_eq!(app.messages().len(), 1);
    assert!(app.messages()[0].body.contains("Spirit"));
}
