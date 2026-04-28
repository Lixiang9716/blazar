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

#[tokio::test]
async fn execute_help_command_adds_timeline_hint() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let baseline_len = app.timeline().len();

    let result = execute_command_for_test(&mut app, "/help", json!({}))
        .await
        .expect("help command should execute");

    assert_eq!(result.summary, "Help information displayed");
    assert!(app.timeline().len() > baseline_len);
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("Command reference"))
    );
}

#[tokio::test]
async fn execute_context_command_adds_timeline_hint() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let baseline_len = app.timeline().len();

    let result = execute_command_for_test(&mut app, "/context", json!({}))
        .await
        .expect("context command should execute");

    assert_eq!(result.summary, "Context information displayed");
    assert!(app.timeline().len() > baseline_len);
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("Context window"))
    );
}

#[tokio::test]
async fn execute_tools_command_adds_timeline_hint() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let baseline_len = app.timeline().len();

    let result = execute_command_for_test(&mut app, "/tools", json!({}))
        .await
        .expect("tools command should execute");

    assert_eq!(result.summary, "Tools information displayed");
    assert!(app.timeline().len() > baseline_len);
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("Tool listing"))
    );
}

#[tokio::test]
async fn execute_agents_command_adds_timeline_hint() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let baseline_len = app.timeline().len();

    let result = execute_command_for_test(&mut app, "/agents", json!({}))
        .await
        .expect("agents command should execute");

    assert_eq!(result.summary, "Agent information displayed");
    assert!(app.timeline().len() > baseline_len);
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("Agent status"))
    );
}

#[tokio::test]
async fn execute_skills_command_adds_timeline_hint() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let baseline_len = app.timeline().len();

    let result = execute_command_for_test(&mut app, "/skills", json!({}))
        .await
        .expect("skills command should execute");

    assert_eq!(result.summary, "Skills information displayed");
    assert!(app.timeline().len() > baseline_len);
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("Skills listing"))
    );
}

#[tokio::test]
async fn execute_history_command_adds_timeline_hint() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let baseline_len = app.timeline().len();

    let result = execute_command_for_test(&mut app, "/history", json!({}))
        .await
        .expect("history command should execute");

    assert_eq!(result.summary, "History view displayed");
    assert!(app.timeline().len() > baseline_len);
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("history browser"))
    );
}

#[tokio::test]
async fn execute_config_command_adds_timeline_hint() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let baseline_len = app.timeline().len();

    let result = execute_command_for_test(&mut app, "/config", json!({}))
        .await
        .expect("config command should execute");

    assert_eq!(result.summary, "Config view displayed");
    assert!(app.timeline().len() > baseline_len);
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("Configuration settings"))
    );
}

#[tokio::test]
async fn execute_mcp_command_adds_timeline_hint() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let baseline_len = app.timeline().len();

    let result = execute_command_for_test(&mut app, "/mcp", json!({}))
        .await
        .expect("mcp command should execute");

    assert_eq!(result.summary, "MCP config view displayed");
    assert!(app.timeline().len() > baseline_len);
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("MCP server configuration"))
    );
}

#[tokio::test]
async fn execute_terminal_command_adds_timeline_hint() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let baseline_len = app.timeline().len();

    let result = execute_command_for_test(&mut app, "/terminal", json!({}))
        .await
        .expect("terminal command should execute");

    assert_eq!(result.summary, "Terminal view displayed");
    assert!(app.timeline().len() > baseline_len);
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("Terminal shell"))
    );
}

#[tokio::test]
async fn execute_log_command_adds_timeline_hint() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let baseline_len = app.timeline().len();

    let result = execute_command_for_test(&mut app, "/log", json!({}))
        .await
        .expect("log command should execute");

    assert_eq!(result.summary, "Log view displayed");
    assert!(app.timeline().len() > baseline_len);
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("logs viewer"))
    );
}
