// Tests for SessionSummary::load_from_dir() — uses a temp session dir fixture.
use blazar::chat::session::SessionSummary;
use rusqlite::Connection;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nanos}"))
}

fn create_session_fixture(base: &PathBuf) {
    std::fs::create_dir_all(base.join("checkpoints")).unwrap();

    std::fs::write(
        base.join("workspace.yaml"),
        "label: test-session\nrepoPath: /home/user/project\n",
    )
    .unwrap();

    std::fs::write(base.join("plan.md"), "# Plan\n## Tasks\n- [ ] task one\n").unwrap();

    std::fs::write(
        base.join("checkpoints/index.md"),
        "# Checkpoints\n- Checkpoint 001: Start\n- Checkpoint 002: Middle\n- Checkpoint 003: End\n",
    )
    .unwrap();

    let conn = Connection::open(base.join("session.db")).unwrap();
    conn.execute_batch(
        "CREATE TABLE todos (
            id TEXT PRIMARY KEY,
            title TEXT,
            description TEXT,
            status TEXT,
            created_at TEXT,
            updated_at TEXT
        );
        INSERT INTO todos VALUES ('t1','Task A','d','pending','','');
        INSERT INTO todos VALUES ('t2','Task B','d','pending','','');
        INSERT INTO todos VALUES ('t3','Task C','d','in_progress','','');
        INSERT INTO todos VALUES ('t4','Task D','d','done','','');
        INSERT INTO todos VALUES ('t5','Task E','d','done','','');
        INSERT INTO todos VALUES ('t6','Task F','d','done','','');",
    )
    .unwrap();
}

#[test]
fn session_loader_reads_label_from_workspace_yaml() {
    let dir = unique_dir("blazar-session-label");
    create_session_fixture(&dir);

    let summary = SessionSummary::load_from_dir(std::path::Path::new("."), Some(&dir));

    assert_eq!(
        summary.session_label, "test-session",
        "should read label from workspace.yaml, got: {}",
        summary.session_label
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn session_loader_detects_plan_md_present() {
    let dir = unique_dir("blazar-session-plan");
    create_session_fixture(&dir);

    let summary = SessionSummary::load_from_dir(std::path::Path::new("."), Some(&dir));

    assert!(
        summary.plan_status.contains("plan.md"),
        "plan_status should mention plan.md when present, got: {}",
        summary.plan_status
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn session_loader_reads_checkpoints_from_index() {
    let dir = unique_dir("blazar-session-checkpoints");
    create_session_fixture(&dir);

    let summary = SessionSummary::load_from_dir(std::path::Path::new("."), Some(&dir));

    assert!(
        !summary.checkpoints.is_empty(),
        "should load checkpoints from index.md"
    );
    assert!(
        summary.checkpoints.iter().any(|c| c.contains("Checkpoint 001")),
        "should include Checkpoint 001, got: {:?}",
        summary.checkpoints
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn session_loader_counts_todos_by_status() {
    let dir = unique_dir("blazar-session-todos");
    create_session_fixture(&dir);

    let summary = SessionSummary::load_from_dir(std::path::Path::new("."), Some(&dir));

    assert_eq!(summary.ready_todos, 2, "pending=2 should map to ready_todos");
    assert_eq!(summary.in_progress_todos, 1, "in_progress_todos should be 1");
    assert_eq!(summary.done_todos, 3, "done_todos should be 3");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn session_loader_returns_empty_summary_when_dir_missing() {
    let dir = PathBuf::from("/nonexistent/session/dir/xyz");

    let summary = SessionSummary::load_from_dir(std::path::Path::new("."), Some(&dir));

    assert!(
        summary.session_label.is_empty(),
        "missing session dir should yield empty summary"
    );
}

#[test]
fn session_loader_returns_empty_summary_when_dir_is_none() {
    let summary = SessionSummary::load_from_dir(std::path::Path::new("."), None);

    assert!(
        summary.session_label.is_empty(),
        "no session dir should yield empty summary"
    );
    assert_eq!(summary.ready_todos, 0);
    assert_eq!(summary.done_todos, 0);
}

#[test]
fn session_loader_fallback_plan_status_when_no_plan_md() {
    let dir = unique_dir("blazar-session-noplan");
    std::fs::create_dir_all(dir.join("checkpoints")).unwrap();
    std::fs::write(dir.join("workspace.yaml"), "label: no-plan-session\n").unwrap();

    let summary = SessionSummary::load_from_dir(std::path::Path::new("."), Some(&dir));

    assert!(
        summary.plan_status.contains("No plan"),
        "should show 'No plan' when plan.md absent, got: {}",
        summary.plan_status
    );
    std::fs::remove_dir_all(&dir).ok();
}

// Issue 1: Connection::open must not silently create session.db when it is absent.
#[test]
fn load_from_dir_does_not_create_session_db_when_absent() {
    let dir = unique_dir("blazar-no-create-db");
    std::fs::create_dir_all(&dir).unwrap();
    // Provide workspace.yaml so the loader proceeds past the early-exit guards,
    // but deliberately omit session.db.
    std::fs::write(dir.join("workspace.yaml"), "label: no-db-session\n").unwrap();

    let db_path = dir.join("session.db");
    assert!(!db_path.exists(), "session.db must not exist before the call");

    let _summary = SessionSummary::load_from_dir(std::path::Path::new("."), Some(&dir));

    assert!(
        !db_path.exists(),
        "load_from_dir must NOT create session.db when the file is absent"
    );
    std::fs::remove_dir_all(&dir).ok();
}
