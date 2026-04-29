use blazar::chat::app::ChatApp;
use blazar::chat::commands::orchestrator::execute_palette_command;
use blazar::chat::commands::{CommandError, CommandRegistry};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;
use std::path::{Path, PathBuf};

const PLAN_CONTINUATION_LIMIT: usize = 8;

async fn execute_command_for_test(
    app: &mut ChatApp,
    name: &str,
    args: serde_json::Value,
) -> Result<blazar::chat::commands::CommandResult, CommandError> {
    let registry = CommandRegistry::with_builtins().expect("bootstrap built-ins");
    execute_palette_command(&registry, app, name, args).await
}

fn run_git_command(workspace: &Path, args: &[&str]) {
    let command = format!("git {}", args.join(" "));
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(workspace)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "failed to run `{command}` in {}: {error}",
                workspace.display()
            )
        });

    assert!(
        output.status.success(),
        "`{command}` failed in {} (exit: {:?})\nstdout:\n{}\nstderr:\n{}",
        workspace.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

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

    run_git_command(&workspace, &["init"]);
    run_git_command(&workspace, &["config", "user.email", "test@example.com"]);
    run_git_command(&workspace, &["config", "user.name", "Test User"]);

    workspace
}

fn latest_plan_id(workspace: &Path) -> String {
    let plans_dir = workspace.join(".blazar").join("plans");
    let mut ids = std::fs::read_dir(plans_dir)
        .expect("plans dir should exist")
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                return None;
            }
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids.pop().expect("at least one plan should exist")
}

fn read_plan_json(workspace: &Path, plan_id: &str) -> serde_json::Value {
    let plan_path = workspace
        .join(".blazar")
        .join("plans")
        .join(format!("{plan_id}.json"));
    let raw = std::fs::read_to_string(plan_path).expect("plan session json should exist");
    serde_json::from_str(&raw).expect("plan json should parse")
}

fn plan_index_path(workspace: &Path) -> PathBuf {
    workspace
        .join(".blazar")
        .join("state")
        .join("plan_index.db")
}

fn read_index_phase_and_status(index_path: &Path, plan_id: &str) -> Option<(String, String)> {
    if !index_path.exists() {
        return None;
    }

    let conn = Connection::open(index_path).expect("index db should open");
    conn.query_row(
        "SELECT phase, status FROM plans WHERE plan_id = ?1",
        params![plan_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )
    .optional()
    .expect("index query should succeed")
}

#[tokio::test]
async fn plan_segmented_flow_reaches_done_state() {
    let workspace = create_unique_test_workspace("plan_segmented_done");
    let mut app = ChatApp::new_for_test(workspace.to_str().unwrap()).expect("app");

    execute_command_for_test(
        &mut app,
        "/plan",
        json!({"goal": "ship segmented /plan workflow"}),
    )
    .await
    .expect("initial /plan should execute");

    let plan_id = latest_plan_id(&workspace);
    assert_eq!(app.composer_text(), "/plan");
    for _ in 0..PLAN_CONTINUATION_LIMIT {
        let saved = read_plan_json(&workspace, &plan_id);
        let phase = saved
            .get("phase")
            .and_then(serde_json::Value::as_str)
            .expect("phase should exist");

        if phase == "Done" {
            break;
        }

        let mut args = json!({"continue_id": plan_id.as_str()});
        if phase == "Review" {
            args["review"] = json!("continue");
        }

        execute_command_for_test(&mut app, "/plan", args)
            .await
            .expect("continuation should execute");
    }

    let saved = read_plan_json(&workspace, &plan_id);
    let final_phase = saved.get("phase").and_then(serde_json::Value::as_str);
    let final_status = saved.get("status").and_then(serde_json::Value::as_str);
    assert!(
        final_phase == Some("Done"),
        "segmented /plan flow should reach Done within {PLAN_CONTINUATION_LIMIT} continuations; final phase={final_phase:?}, final status={final_status:?}"
    );
    assert_eq!(
        final_status,
        Some("completed"),
        "done plan should have completed status"
    );

    let _ = std::fs::remove_dir_all(&workspace);
}

#[tokio::test]
async fn plan_rebuilds_sqlite_index_from_json_when_stale_or_missing() {
    let workspace = create_unique_test_workspace("plan_index_rebuild_json");
    let mut app = ChatApp::new_for_test(workspace.to_str().unwrap()).expect("app");

    execute_command_for_test(&mut app, "/plan", json!({"goal": "verify index rebuild"}))
        .await
        .expect("initial /plan should execute");

    let plan_id = latest_plan_id(&workspace);
    assert_eq!(app.composer_text(), "/plan");
    let index_path = plan_index_path(&workspace);

    {
        let conn = Connection::open(&index_path).expect("index db should open for stale fixture");
        conn.execute(
            "UPDATE plans SET phase = ?2, status = ?3 WHERE plan_id = ?1",
            params![plan_id, "StalePhase", "stale"],
        )
        .expect("stale fixture update should succeed");
    }

    assert_eq!(
        read_index_phase_and_status(&index_path, &plan_id),
        Some(("StalePhase".to_owned(), "stale".to_owned())),
        "fixture should make index stale before rebuild"
    );

    execute_command_for_test(&mut app, "/plan", json!({"continue_id": plan_id.as_str()}))
        .await
        .expect("continuation should repair stale index row");

    let after_stale_repair = read_plan_json(&workspace, &plan_id);
    let repaired_phase = after_stale_repair
        .get("phase")
        .and_then(serde_json::Value::as_str)
        .expect("phase should exist after stale repair")
        .to_owned();
    let repaired_status = after_stale_repair
        .get("status")
        .and_then(serde_json::Value::as_str)
        .expect("status should exist after stale repair")
        .to_owned();
    assert_eq!(
        read_index_phase_and_status(&index_path, &plan_id),
        Some((repaired_phase.clone(), repaired_status.clone())),
        "stale sqlite row should be rebuilt from JSON source-of-truth"
    );

    std::fs::remove_file(&index_path)
        .expect("index db should be removable to simulate missing index");
    assert!(
        !index_path.exists(),
        "index db should be absent before missing-index recovery"
    );

    execute_command_for_test(&mut app, "/plan", json!({"continue_id": plan_id.as_str()}))
        .await
        .expect("continuation should recreate missing index from JSON");

    let after_missing_repair = read_plan_json(&workspace, &plan_id);
    let rebuilt_phase = after_missing_repair
        .get("phase")
        .and_then(serde_json::Value::as_str)
        .expect("phase should exist after missing-index repair")
        .to_owned();
    let rebuilt_status = after_missing_repair
        .get("status")
        .and_then(serde_json::Value::as_str)
        .expect("status should exist after missing-index repair")
        .to_owned();
    assert!(
        index_path.exists(),
        "missing sqlite index should be recreated by /plan continuation"
    );
    assert_eq!(
        read_index_phase_and_status(&index_path, &plan_id),
        Some((rebuilt_phase, rebuilt_status)),
        "recreated sqlite index should be rebuilt from JSON source-of-truth"
    );

    let _ = std::fs::remove_dir_all(&workspace);
}
