use blazar::chat::app::ChatApp;
use blazar::chat::commands::orchestrator::execute_palette_command;
use blazar::chat::commands::{CommandError, CommandRegistry};
use blazar::chat::model::Actor;
use serde_json::json;
use std::path::{Path, PathBuf};

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

fn extract_continue_plan_id(composer_text: &str) -> &str {
    composer_text
        .strip_prefix("/plan --continue ")
        .expect("composer should contain continue command")
}

fn read_plan_json(workspace: &Path, plan_id: &str) -> serde_json::Value {
    let plan_path = workspace
        .join(".blazar")
        .join("plans")
        .join(format!("{plan_id}.json"));
    let raw = std::fs::read_to_string(plan_path).expect("plan session json should exist");
    serde_json::from_str(&raw).expect("plan json should parse")
}

fn write_plan_json(workspace: &Path, plan_id: &str, payload: serde_json::Value) {
    let plans_dir = workspace.join(".blazar").join("plans");
    std::fs::create_dir_all(&plans_dir).expect("plan directory should exist");
    std::fs::write(
        plans_dir.join(format!("{plan_id}.json")),
        serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    )
    .expect("fixture plan json should write");
}

#[tokio::test]
async fn execute_plan_command_bootstraps_session_and_sets_continue_guidance() {
    let workspace = create_unique_test_workspace("plan_bootstrap");
    let mut app = ChatApp::new_for_test(workspace.to_str().unwrap()).expect("app");

    let result = execute_command_for_test(&mut app, "/plan", json!({"goal": "wire orchestration"}))
        .await
        .expect("plan command should execute");

    let composer = app.composer_text();
    let plan_id = extract_continue_plan_id(&composer);
    assert!(!plan_id.is_empty(), "continue id should be present");
    assert!(
        result.summary.contains(plan_id),
        "summary should mention plan id"
    );

    let saved = read_plan_json(&workspace, plan_id);
    assert_eq!(
        saved.get("goal").and_then(serde_json::Value::as_str),
        Some("wire orchestration")
    );
    assert_eq!(
        saved.get("phase").and_then(serde_json::Value::as_str),
        Some("DraftStep")
    );
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.contains("/plan --continue") && entry.body.contains(plan_id)),
        "timeline should include continue hint"
    );
    assert!(
        app.timeline()
            .iter()
            .any(|entry| entry.body.to_ascii_lowercase().contains("deferred")),
        "timeline guidance should explicitly call out deferred continuation semantics"
    );

    let _ = std::fs::remove_dir_all(&workspace);
}

#[tokio::test]
async fn execute_plan_command_without_goal_enters_clarify_phase() {
    let workspace = create_unique_test_workspace("plan_clarify");
    let mut app = ChatApp::new_for_test(workspace.to_str().unwrap()).expect("app");

    let result = execute_command_for_test(&mut app, "/plan", json!({}))
        .await
        .expect("plan command should execute");

    let plan_id = extract_continue_plan_id(&app.composer_text()).to_owned();
    let saved = read_plan_json(&workspace, &plan_id);

    assert!(result.summary.contains("Clarify"));
    assert_eq!(
        saved.get("phase").and_then(serde_json::Value::as_str),
        Some("Clarify")
    );

    let _ = std::fs::remove_dir_all(&workspace);
}

#[tokio::test]
async fn execute_plan_command_continue_id_loads_and_advances_existing_session() {
    let workspace = create_unique_test_workspace("plan_continue");
    let mut app = ChatApp::new_for_test(workspace.to_str().unwrap()).expect("app");

    execute_command_for_test(&mut app, "/plan", json!({}))
        .await
        .expect("initial bootstrap should succeed");
    let plan_id = extract_continue_plan_id(&app.composer_text()).to_owned();

    let result = execute_command_for_test(
        &mut app,
        "/plan",
        json!({"continue_id": plan_id, "goal": "finish command orchestration"}),
    )
    .await
    .expect("continue command should execute");

    assert!(
        result.summary.contains("continued"),
        "continue summary should communicate resumed execution"
    );
    let saved = read_plan_json(&workspace, extract_continue_plan_id(&app.composer_text()));
    assert_eq!(
        saved.get("goal").and_then(serde_json::Value::as_str),
        Some("finish command orchestration")
    );
    assert_eq!(
        saved.get("phase").and_then(serde_json::Value::as_str),
        Some("DraftStep")
    );

    let _ = std::fs::remove_dir_all(&workspace);
}

#[tokio::test]
async fn execute_plan_command_continue_id_handles_review_transitions() {
    let workspace = create_unique_test_workspace("plan_review_transitions");
    let mut app = ChatApp::new_for_test(workspace.to_str().unwrap()).expect("app");

    let continue_id = "plan-review-continue";
    write_plan_json(
        &workspace,
        continue_id,
        json!({
            "id": continue_id,
            "phase": "Review",
            "status": "executing",
            "goal": "ship segmented workflow",
            "current_step": 0,
            "steps": [
                {"title": "step-1", "status": "done"},
                {"title": "step-2", "status": "pending"}
            ],
            "events": []
        }),
    );

    execute_command_for_test(
        &mut app,
        "/plan",
        json!({"continue_id": continue_id, "review": "continue"}),
    )
    .await
    .expect("continue command should execute for review continue");
    let saved_continue = read_plan_json(&workspace, continue_id);
    assert_eq!(
        saved_continue
            .get("phase")
            .and_then(serde_json::Value::as_str),
        Some("ExecuteStep")
    );

    let retry_id = "plan-review-retry";
    write_plan_json(
        &workspace,
        retry_id,
        json!({
            "id": retry_id,
            "phase": "Review",
            "status": "executing",
            "goal": "ship segmented workflow",
            "current_step": 0,
            "steps": [{"title": "step-1", "status": "done"}],
            "events": []
        }),
    );

    execute_command_for_test(
        &mut app,
        "/plan",
        json!({"continue_id": retry_id, "review": "retry"}),
    )
    .await
    .expect("continue command should execute for review retry");
    let saved_retry = read_plan_json(&workspace, retry_id);
    assert_eq!(
        saved_retry.get("phase").and_then(serde_json::Value::as_str),
        Some("ExecuteStep")
    );
    assert_eq!(
        saved_retry
            .get("steps")
            .and_then(serde_json::Value::as_array)
            .and_then(|steps| steps.first())
            .and_then(|step| step.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("pending")
    );

    let revise_id = "plan-review-revise";
    write_plan_json(
        &workspace,
        revise_id,
        json!({
            "id": revise_id,
            "phase": "Review",
            "status": "executing",
            "goal": "ship segmented workflow",
            "current_step": 0,
            "steps": [{"title": "step-1", "status": "done"}],
            "events": []
        }),
    );

    execute_command_for_test(
        &mut app,
        "/plan",
        json!({"continue_id": revise_id, "review": "revise"}),
    )
    .await
    .expect("continue command should execute for review revise");
    let saved_revise = read_plan_json(&workspace, revise_id);
    assert_eq!(
        saved_revise
            .get("phase")
            .and_then(serde_json::Value::as_str),
        Some("DraftStep")
    );
    execute_command_for_test(&mut app, "/plan", json!({"continue_id": revise_id}))
        .await
        .expect("revise continuation should draft a replacement micro-step");
    let drafted_revise = read_plan_json(&workspace, revise_id);
    let drafted_steps = drafted_revise
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .expect("drafted revise plan should have steps");
    assert_eq!(
        drafted_revise
            .get("phase")
            .and_then(serde_json::Value::as_str),
        Some("FinalizePlan")
    );
    assert_eq!(
        drafted_steps.len(),
        2,
        "revise should prepare a newly drafted micro-step"
    );
    assert_eq!(
        drafted_revise
            .get("current_step")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        drafted_steps
            .get(1)
            .and_then(|step| step.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("pending")
    );

    execute_command_for_test(&mut app, "/plan", json!({"continue_id": revise_id}))
        .await
        .expect("revise continuation should stage execution for replacement micro-step");
    let execute_ready_revise = read_plan_json(&workspace, revise_id);
    assert_eq!(
        execute_ready_revise
            .get("phase")
            .and_then(serde_json::Value::as_str),
        Some("ExecuteStep")
    );
    assert_eq!(
        execute_ready_revise
            .get("current_step")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );

    execute_command_for_test(&mut app, "/plan", json!({"continue_id": revise_id}))
        .await
        .expect("revise continuation should execute the newly drafted step");
    let executed_revise = read_plan_json(&workspace, revise_id);
    assert_eq!(
        executed_revise
            .get("phase")
            .and_then(serde_json::Value::as_str),
        Some("Review")
    );
    assert_eq!(
        executed_revise
            .get("steps")
            .and_then(serde_json::Value::as_array)
            .and_then(|steps| steps.get(1))
            .and_then(|step| step.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("done"),
        "newly drafted revise step should be executed"
    );

    let cancel_id = "plan-review-cancel";
    write_plan_json(
        &workspace,
        cancel_id,
        json!({
            "id": cancel_id,
            "phase": "Review",
            "status": "executing",
            "goal": "ship segmented workflow",
            "current_step": 0,
            "steps": [{"title": "step-1", "status": "done"}],
            "events": []
        }),
    );

    execute_command_for_test(
        &mut app,
        "/plan",
        json!({"continue_id": cancel_id, "review": "cancel"}),
    )
    .await
    .expect("continue command should execute for review cancel");
    let saved_cancel = read_plan_json(&workspace, cancel_id);
    assert_eq!(
        saved_cancel
            .get("phase")
            .and_then(serde_json::Value::as_str),
        Some("Done")
    );
    assert_eq!(
        saved_cancel
            .get("status")
            .and_then(serde_json::Value::as_str),
        Some("cancelled")
    );

    let _ = std::fs::remove_dir_all(&workspace);
}

#[tokio::test]
async fn execute_plan_command_does_not_replay_review_decision_recorded_outside_review_phase() {
    let workspace = create_unique_test_workspace("plan_review_stale_replay");
    let mut app = ChatApp::new_for_test(workspace.to_str().unwrap()).expect("app");

    let continue_id = "plan-review-stale";
    write_plan_json(
        &workspace,
        continue_id,
        json!({
            "id": continue_id,
            "phase": "ExecuteStep",
            "status": "executing",
            "goal": "ship segmented workflow",
            "current_step": 0,
            "steps": [
                {"title": "step-1", "status": "pending"},
                {"title": "step-2", "status": "pending"}
            ],
            "events": []
        }),
    );

    execute_command_for_test(
        &mut app,
        "/plan",
        json!({"continue_id": continue_id, "review": "revise"}),
    )
    .await
    .expect("execute step continuation should execute");
    let after_execute_step = read_plan_json(&workspace, continue_id);
    assert_eq!(
        after_execute_step
            .get("phase")
            .and_then(serde_json::Value::as_str),
        Some("Review")
    );

    execute_command_for_test(&mut app, "/plan", json!({"continue_id": continue_id}))
        .await
        .expect("review continuation should execute");
    let after_review = read_plan_json(&workspace, continue_id);
    assert_eq!(
        after_review
            .get("phase")
            .and_then(serde_json::Value::as_str),
        Some("ExecuteStep"),
        "review decisions supplied outside the Review phase must not be replayed later"
    );

    let _ = std::fs::remove_dir_all(&workspace);
}

#[tokio::test]
async fn execute_plan_command_rejects_invalid_args() {
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("app");

    let err = execute_command_for_test(&mut app, "/plan", json!({"goal": 123}))
        .await
        .expect_err("non-string goal should fail");

    assert!(matches!(err, CommandError::InvalidArgs(_)));

    let err = execute_command_for_test(&mut app, "/plan", json!({"review": "maybe"}))
        .await
        .expect_err("unsupported review choice should fail");

    assert!(matches!(err, CommandError::InvalidArgs(_)));
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
