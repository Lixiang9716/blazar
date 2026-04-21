use blazar::chat::session::SessionSummary;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn unique_session_dir() -> PathBuf {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&base).expect("target dir should exist");
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    base.join(format!("chat-session-test-{suffix}"))
}

#[test]
fn load_from_dir_returns_default_for_missing_session_dir() {
    let missing = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/session-missing");
    let summary = SessionSummary::load_from_dir(Path::new("."), Some(&missing));

    assert_eq!(summary.session_label, "");
    assert_eq!(summary.cwd, "");
    assert_eq!(summary.active_intent, "");
    assert_eq!(summary.ready_todos, 0);
}

#[test]
fn load_from_dir_reads_workspace_files_todos_and_intent() {
    let session_dir = unique_session_dir();
    std::fs::create_dir_all(session_dir.join("checkpoints")).expect("create checkpoints dir");
    std::fs::write(
        session_dir.join("workspace.yaml"),
        "label: coding-session\nrepoPath: /workspace/blazar\n",
    )
    .expect("write workspace yaml");
    std::fs::write(session_dir.join("plan.md"), "# plan").expect("write plan");
    std::fs::write(
        session_dir.join("checkpoints/index.md"),
        "- Checkpoint 001: Setup\n* Checkpoint 002: Done\n",
    )
    .expect("write checkpoints index");
    std::fs::write(
        session_dir.join("events.jsonl"),
        concat!(
            r#"{"toolName":"other","toolArgs":{"intent":"ignored"}}"#,
            "\n",
            r#"{"toolName":"report_intent","toolResult":{"sessionLog":"fallback intent"}}"#,
            "\n"
        ),
    )
    .expect("write events log");

    let db = Connection::open(session_dir.join("session.db")).expect("open session db");
    db.execute("CREATE TABLE todos (id TEXT, status TEXT)", [])
        .expect("create todos table");
    db.execute("INSERT INTO todos (id, status) VALUES ('a', 'pending')", [])
        .expect("insert pending");
    db.execute(
        "INSERT INTO todos (id, status) VALUES ('b', 'in_progress')",
        [],
    )
    .expect("insert in_progress");
    db.execute("INSERT INTO todos (id, status) VALUES ('c', 'done')", [])
        .expect("insert done");

    let summary = SessionSummary::load_from_dir(Path::new("."), Some(&session_dir));
    assert_eq!(summary.session_label, "coding-session");
    assert_eq!(summary.cwd, "/workspace/blazar");
    assert_eq!(summary.active_intent, "fallback intent");
    assert_eq!(summary.plan_status, "plan.md present");
    assert_eq!(
        summary.checkpoints,
        vec!["Checkpoint 001".to_string(), "Checkpoint 002".to_string()]
    );
    assert_eq!(summary.ready_todos, 1);
    assert_eq!(summary.in_progress_todos, 1);
    assert_eq!(summary.done_todos, 1);

    std::fs::remove_dir_all(session_dir).expect("cleanup session dir");
}

#[test]
fn for_test_returns_stable_seed_data() {
    let summary = SessionSummary::for_test();
    assert_eq!(summary.session_label, "spirit-workspace-tui");
    assert_eq!(summary.cwd, "/home/lx/blazar");
    assert!(summary.ready_todos > 0);
    assert!(!summary.checkpoints.is_empty());
}

#[test]
fn load_uses_session_id_environment_and_prefers_tool_args_intent() {
    let _guard = ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock");

    let home_dir = unique_session_dir();
    let session_id = "session-env-test";
    let session_dir = home_dir.join(".copilot/session-state").join(session_id);
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    std::fs::write(
        session_dir.join("workspace.yaml"),
        "label: env-session\nrepoPath: /env/repo\n",
    )
    .expect("write workspace yaml");
    std::fs::write(
        session_dir.join("events.jsonl"),
        r#"{"toolName":"report_intent","toolArgs":{"intent":"Writing tests"}}"#,
    )
    .expect("write events");

    let old_home = std::env::var("HOME").ok();
    let old_session = std::env::var("COPILOT_AGENT_SESSION_ID").ok();
    unsafe {
        std::env::set_var("HOME", &home_dir);
        std::env::set_var("COPILOT_AGENT_SESSION_ID", session_id);
    }

    let summary = SessionSummary::load(Path::new("."));
    assert_eq!(summary.session_label, "env-session");
    assert_eq!(summary.cwd, "/env/repo");
    assert_eq!(summary.active_intent, "Writing tests");
    assert_eq!(summary.plan_status, "No plan");

    unsafe {
        match old_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        match old_session {
            Some(value) => std::env::set_var("COPILOT_AGENT_SESSION_ID", value),
            None => std::env::remove_var("COPILOT_AGENT_SESSION_ID"),
        }
    }
    std::fs::remove_dir_all(home_dir).expect("cleanup home dir");
}
