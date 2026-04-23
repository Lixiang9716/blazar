use blazar::chat::app::ChatApp;
use blazar::chat::commands::CommandError;
use blazar::chat::commands::orchestrator::execute_palette_command_for_test;
use blazar::chat::model::Actor;
use serde_json::json;

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[tokio::test]
async fn execute_plan_command_sets_composer_prefill() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");

    execute_palette_command_for_test(&mut app, "/plan", json!({}))
        .await
        .expect("plan command should execute");

    assert_eq!(app.composer_text(), "/plan ");
}

#[tokio::test]
async fn unknown_command_returns_unavailable_error() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");

    let err = execute_palette_command_for_test(&mut app, "/does-not-exist", json!({}))
        .await
        .expect_err("unknown command should fail");

    assert!(matches!(
        err,
        CommandError::Unavailable(message) if message.contains("/does-not-exist")
    ));
}

#[tokio::test]
async fn execute_discover_agents_command_adds_timeline_hint() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");

    execute_palette_command_for_test(&mut app, "/discover-agents", json!({}))
        .await
        .expect("discover agents command should execute");

    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == "/discover-agents")
    );
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("Discovering ACP agents"))
    );
}
