use blazar::chat::app::ChatApp;
use blazar::chat::commands::orchestrator::execute_palette_command;
use blazar::chat::commands::{CommandError, CommandRegistry};
use blazar::chat::model::Actor;
use serde_json::json;
use std::path::PathBuf;

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

async fn execute_command_for_test(
    app: &mut ChatApp,
    name: &str,
    args: serde_json::Value,
) -> Result<blazar::chat::commands::CommandResult, CommandError> {
    let registry = CommandRegistry::with_builtins().expect("bootstrap built-ins");
    execute_palette_command(&registry, app, name, args).await
}

/// Creates a unique temporary test workspace directory.
/// Returns a PathBuf that will be automatically cleaned up when dropped.
fn create_unique_test_workspace(test_name: &str) -> PathBuf {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-workspaces");
    std::fs::create_dir_all(&base).expect("create test-workspaces dir");

    let unique = format!(
        "{}-{}",
        test_name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos()
    );

    let workspace = base.join(unique);
    std::fs::create_dir_all(&workspace).expect("create unique workspace");

    // Initialize as a git repo to satisfy ChatApp requirements
    let _ = std::process::Command::new("git")
        .args(["init"])
        .current_dir(&workspace)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&workspace)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&workspace)
        .output();

    workspace
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
async fn execute_compact_command_starts_compaction_flow() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");

    let result = execute_command_for_test(&mut app, "/compact", json!({}))
        .await
        .expect("compact command should execute");

    assert_eq!(result.summary, "Compaction started");
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.actor == Actor::User && entry.body == "/compact"),
        "should queue /compact message"
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

// Task 5 command tests

#[tokio::test]
async fn execute_copy_command_without_messages_returns_unavailable() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");

    let err = execute_command_for_test(&mut app, "/copy", json!({}))
        .await
        .expect_err("copy should fail without assistant messages");

    assert!(matches!(
        err,
        CommandError::Unavailable(message) if message.contains("No assistant message")
    ));
}

#[tokio::test]
async fn execute_init_command_creates_instructions_file() {
    let workspace = create_unique_test_workspace("init_creates");
    let mut app = ChatApp::new_for_test(workspace.to_str().unwrap()).expect("app");
    let instructions_path = app.workspace_root().join("blazar-instructions.md");

    let result = execute_command_for_test(&mut app, "/init", json!({}))
        .await
        .expect("init command should execute");

    assert!(result.summary.contains("Created"));
    assert!(instructions_path.exists());
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("Created blazar-instructions.md"))
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&workspace);
}

#[tokio::test]
async fn execute_init_command_when_file_exists() {
    let workspace = create_unique_test_workspace("init_exists");
    let mut app = ChatApp::new_for_test(workspace.to_str().unwrap()).expect("app");
    let instructions_path = app.workspace_root().join("blazar-instructions.md");

    // Create the file first
    std::fs::write(&instructions_path, "existing content").expect("write file");

    let result = execute_command_for_test(&mut app, "/init", json!({}))
        .await
        .expect("init command should execute");

    assert!(result.summary.contains("already exists"));
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("already exists"))
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&workspace);
}

#[tokio::test]
async fn execute_git_command_shows_status() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let baseline_len = app.timeline().len();

    let result = execute_command_for_test(&mut app, "/git", json!({}))
        .await
        .expect("git command should execute");

    assert!(result.summary.contains("clean") || result.summary.contains("changed"));
    assert!(app.timeline().len() > baseline_len);
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("branch:"))
    );
}

#[tokio::test]
async fn execute_diff_command_shows_changes_or_none() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");

    let result = execute_command_for_test(&mut app, "/diff", json!({}))
        .await
        .expect("diff command should execute");

    assert!(result.summary.contains("No changes") || result.summary.contains("changed"));
}

#[tokio::test]
async fn execute_undo_command_shows_hint() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");
    let baseline_len = app.timeline().len();

    let result = execute_command_for_test(&mut app, "/undo", json!({}))
        .await
        .expect("undo command should execute");

    assert_eq!(result.summary, "Undo hint displayed");
    assert!(app.timeline().len() > baseline_len);
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("undo tracking"))
    );
}

#[tokio::test]
async fn execute_export_command_creates_json_file() {
    let workspace = create_unique_test_workspace("export");
    let mut app = ChatApp::new_for_test(workspace.to_str().unwrap()).expect("app");

    let result = execute_command_for_test(&mut app, "/export", json!({}))
        .await
        .expect("export command should execute");

    assert!(result.summary.contains("Exported"));
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("Exported conversation"))
    );

    // Verify the file was created
    let exported_files: Vec<_> = std::fs::read_dir(&workspace)
        .expect("read workspace")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|n| n.starts_with("blazar-export-") && n.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect();
    assert!(!exported_files.is_empty(), "export should create a file");

    // Clean up
    let _ = std::fs::remove_dir_all(&workspace);
}
