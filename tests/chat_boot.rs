use blazar::chat::app::ChatApp;

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn chat_app_starts_with_no_messages() {
    let app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

    assert!(app.messages().is_empty());
}

#[test]
fn sending_a_user_message_appends_user_and_spirit_messages() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

    app.send_message("Help me design a Spirit chat UI");

    // User message is added synchronously.
    assert_eq!(app.messages().len(), 1);
    assert!(app.messages()[0].body.contains("Help me design"));

    // Agent response arrives asynchronously via tick().
    // Give the background thread time to process, then drain events.
    std::thread::sleep(std::time::Duration::from_millis(200));
    app.tick();

    // The timeline should now contain the user message and the echo response.
    assert!(
        app.timeline()
            .iter()
            .any(|e| e.body.contains("Help me design"))
    );
    assert!(app.timeline().iter().any(|e| e.body.contains("Echo:")));
}

#[test]
fn composer_submit_moves_text_into_the_timeline() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");

    app.set_composer_text("Show me a warm, cozy theme");
    app.submit_composer();

    assert!(
        app.messages()
            .iter()
            .any(|msg| msg.body.contains("warm, cozy theme"))
    );
    assert_eq!(app.composer_text(), "");
}
