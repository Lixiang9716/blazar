use blazar::chat::git::GitSummary;
use blazar::chat::view::render_workspace_to_lines_for_test;
use blazar::chat::workspace::{WorkspaceApp, WorkspaceView};

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn git_view_renders_branch_and_dirty_status() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);
    app.select_view(WorkspaceView::Git);

    let summary = GitSummary::for_test();
    app.set_git_summary_for_test(summary);

    let lines = render_workspace_to_lines_for_test(&app, 100, 40);

    assert!(
        lines.iter().any(|line| line.contains("main")),
        "git view should show the branch name"
    );
    assert!(
        lines.iter().any(|line| line.contains("dirty") || line.contains("clean")),
        "git view should show clean/dirty status"
    );
    assert!(
        lines.iter().any(|line| line.contains("README.md")),
        "git view should show a changed file"
    );
    assert!(
        lines.iter().any(|line| line.contains("feat: initial commit")),
        "git view should show a recent commit"
    );
}

#[test]
fn git_view_shows_ahead_behind_and_staged_unstaged() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);
    app.select_view(WorkspaceView::Git);

    let summary = GitSummary::for_test();
    app.set_git_summary_for_test(summary);

    let lines = render_workspace_to_lines_for_test(&app, 100, 40);
    let all = lines.join("\n");

    assert!(all.contains("ahead"), "git view should show ahead count");
    assert!(all.contains("behind"), "git view should show behind count");
    assert!(all.contains("staged"), "git view should show staged count");
    assert!(all.contains("unstaged"), "git view should show unstaged count");
}
