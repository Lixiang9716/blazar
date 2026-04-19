use blazar::chat::session::SessionSummary;
use blazar::chat::view::render_workspace_to_lines_for_test;
use blazar::chat::workspace::{WorkspaceApp, WorkspaceView};

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn session_view_renders_label_plan_status_checkpoint_and_todo_counts() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);
    app.select_view(WorkspaceView::Sessions);

    let summary = SessionSummary::for_test();
    app.set_session_summary_for_test(summary);

    let lines = render_workspace_to_lines_for_test(&app, 100, 40);
    let all = lines.join("\n");

    assert!(
        all.contains("spirit-workspace-tui"),
        "session view should show the session label"
    );
    assert!(
        all.contains("plan.md"),
        "session view should show plan status"
    );
    assert!(
        all.contains("Checkpoint 004"),
        "session view should show checkpoint entries"
    );
    assert!(
        all.contains("done:"),
        "session view should show done todo count"
    );
    assert!(
        all.contains("in progress:"),
        "session view should show in-progress todo count"
    );
    assert!(
        all.contains("ready:"),
        "session view should show ready todo count"
    );
}

#[test]
fn session_view_no_metadata_shows_no_session_details() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);
    app.select_view(WorkspaceView::Sessions);

    // Fully empty summary — session_label is empty, takes the early branch.
    app.set_session_summary_for_test(SessionSummary::default());

    let lines = render_workspace_to_lines_for_test(&app, 100, 40);
    let all = lines.join("\n");

    assert!(
        all.contains("No session details available yet"),
        "empty session view should show 'No session details available yet', got:\n{all}"
    );
}

#[test]
fn session_view_no_checkpoints_shows_no_checkpoints_recorded() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);
    app.select_view(WorkspaceView::Sessions);

    // Non-empty session label (skips the early branch) but no checkpoints.
    let summary = SessionSummary {
        session_label: "spirit-workspace-tui".to_string(),
        cwd: "/home/lx/blazar".to_string(),
        active_intent: "Exploring".to_string(),
        plan_status: "plan.md".to_string(),
        checkpoints: vec![],
        ready_todos: 0,
        in_progress_todos: 0,
        done_todos: 0,
    };
    app.set_session_summary_for_test(summary);

    let lines = render_workspace_to_lines_for_test(&app, 100, 40);
    let all = lines.join("\n");

    assert!(
        all.contains("No checkpoints recorded"),
        "session view with no checkpoints should show 'No checkpoints recorded', got:\n{all}"
    );
}
