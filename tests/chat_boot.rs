use blazar::chat::app::ChatApp;

#[test]
fn chat_app_starts_with_a_spirit_greeting_message() {
    let app = ChatApp::new_for_test("/home/lx/blazar");

    assert_eq!(app.messages().len(), 1);
    assert!(app.messages()[0].body.contains("Spirit"));
}

#[test]
fn sending_a_user_message_appends_user_and_spirit_messages() {
    let mut app = ChatApp::new_for_test("/home/lx/blazar");

    app.send_message("Help me design a Spirit chat UI");

    assert_eq!(app.messages().len(), 3);
    assert!(app.messages()[1].body.contains("Help me design"));
    assert!(app.messages()[2].body.contains("Spirit"));
}

#[test]
fn composer_submit_moves_text_into_the_timeline() {
    let mut app = ChatApp::new_for_test("/home/lx/blazar");

    app.set_composer_text("Show me a warm, cozy theme");
    app.submit_composer();

    assert!(
        app.messages()
            .iter()
            .any(|msg| msg.body.contains("warm, cozy theme"))
    );
    assert_eq!(app.composer_text(), "");
}
