use blazar::chat::workspace_catalog::{
    LaunchDecision, StartupPreference, WorkspaceCatalog, WorkspaceRecord,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn startup_prefers_last_opened_path_when_valid_and_launcher_not_forced() {
    let repo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let catalog = WorkspaceCatalog {
        last_opened: Some(repo_path.display().to_string()),
        workspaces: vec![],
    };

    let decision = catalog.decide_startup(StartupPreference {
        repo_path_hint: Some(repo_path.clone()),
        force_launcher: false,
    });

    assert_eq!(
        decision,
        LaunchDecision::Resume {
            repo_path,
            initial_view: None,
        }
    );
}

#[test]
fn startup_prefers_last_opened_before_repo_path_hint_when_both_are_valid() {
    let last_opened = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_path_hint = last_opened
        .parent()
        .expect("workspace root should have a parent")
        .to_path_buf();
    let catalog = WorkspaceCatalog {
        last_opened: Some(last_opened.display().to_string()),
        workspaces: vec![],
    };

    let decision = catalog.decide_startup(StartupPreference {
        repo_path_hint: Some(repo_path_hint),
        force_launcher: false,
    });

    assert_eq!(
        decision,
        LaunchDecision::Resume {
            repo_path: last_opened,
            initial_view: None,
        }
    );
}

#[test]
fn catalog_round_trips_saved_workspaces() {
    let path = unique_catalog_path();
    let catalog = WorkspaceCatalog {
        last_opened: Some("/repo/blazar".to_string()),
        workspaces: vec![WorkspaceRecord {
            name: "blazar".to_string(),
            repo_path: "/repo/blazar".to_string(),
            branch: "main".to_string(),
            dirty: true,
            last_session_label: Some("session-z".to_string()),
            last_intent: Some("Testing persistence".to_string()),
            latest_checkpoint: Some("Checkpoint 011".to_string()),
            ready_todos: 2,
            last_opened_at: 7,
        }],
    };

    catalog.save_to_path(&path).expect("catalog should save");

    let loaded = WorkspaceCatalog::load_from_path(&path);

    assert_eq!(loaded, catalog);
    std::fs::remove_dir_all(path.parent().expect("catalog path has parent")).ok();
}

fn unique_catalog_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("chat-workspace-catalog-{nanos}"))
        .join("workspaces.json")
}
