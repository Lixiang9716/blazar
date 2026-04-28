use blazar::chat::app::ChatApp;
use blazar::chat::commands::orchestrator::execute_palette_command;
use blazar::chat::commands::{CommandError, CommandRegistry};
use blazar::chat::model::Actor;
use serde_json::json;

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

async fn execute_command_for_test(
    app: &mut ChatApp,
    name: &str,
    args: serde_json::Value,
) -> Result<blazar::chat::commands::CommandResult, CommandError> {
    let registry = CommandRegistry::with_builtins().expect("bootstrap built-ins");
    execute_palette_command(&registry, app, name, args).await
}

#[tokio::test]
async fn execute_plan_command_sets_composer_prefill() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");

    execute_command_for_test(&mut app, "/plan", json!({}))
        .await
        .expect("plan command should execute");

    assert_eq!(app.composer_text(), "/plan ");
}

#[tokio::test]
async fn unknown_command_returns_unavailable_error() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let baseline_timeline_len = app.timeline().len();
    let baseline_composer = app.composer_text().to_string();

    let err = execute_command_for_test(&mut app, "/does-not-exist", json!({}))
        .await
        .expect_err("unknown command should fail");

    assert!(matches!(
        err,
        CommandError::Unavailable(message) if message.contains("/does-not-exist")
    ));
    assert_eq!(app.timeline().len(), baseline_timeline_len);
    assert_eq!(app.composer_text(), baseline_composer);
}

#[tokio::test]
async fn execute_discover_agents_command_adds_timeline_hint() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");

    execute_command_for_test(&mut app, "/discover-agents", json!({}))
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

#[tokio::test]
async fn execute_quit_command_forwards_to_app() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");

    let result = execute_command_for_test(&mut app, "/quit", json!({}))
        .await
        .expect("quit command should execute");

    assert_eq!(result.summary, "Queued /quit");
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == "/quit")
    );
}
