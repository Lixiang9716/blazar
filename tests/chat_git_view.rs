use blazar::chat::git::GitSummary;
use blazar::chat::view::render_workspace_to_lines_for_test;
use blazar::chat::workspace::{WorkspaceApp, WorkspaceView};
use blazar::chat::session::SessionSummary;

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
        lines
            .iter()
            .any(|line| line.contains("dirty") || line.contains("clean")),
        "git view should show clean/dirty status"
    );
    assert!(
        lines.iter().any(|line| line.contains("README.md")),
        "git view should show a changed file"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("feat: initial commit")),
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
    assert!(
        all.contains("unstaged"),
        "git view should show unstaged count"
    );
}

#[test]
fn git_view_shows_working_tree_clean_when_no_changed_files() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);
    app.select_view(WorkspaceView::Git);

    let summary = GitSummary {
        changed_files: vec![],
        recent_commits: vec!["feat: some commit".to_string()],
        ..GitSummary::default()
    };
    app.set_git_summary_for_test(summary);

    let lines = render_workspace_to_lines_for_test(&app, 100, 40);
    assert!(
        lines.iter().any(|l| l.contains("Working tree clean")),
        "git view should show 'Working tree clean' when changed_files is empty;\nlines:\n{}",
        lines.join("\n")
    );
}

#[test]
fn git_view_shows_no_recent_commits_when_empty() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);
    app.select_view(WorkspaceView::Git);

    let summary = GitSummary {
        changed_files: vec!["file.rs".to_string()],
        recent_commits: vec![],
        ..GitSummary::default()
    };
    app.set_git_summary_for_test(summary);

    let lines = render_workspace_to_lines_for_test(&app, 100, 40);
    assert!(
        lines.iter().any(|l| l.contains("No recent commits")),
        "git view should show 'No recent commits' when recent_commits is empty;\nlines:\n{}",
        lines.join("\n")
    );
}

#[test]
fn git_view_non_chat_footer_shows_workspace_hints() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);
    app.select_view(WorkspaceView::Git);

    let lines = render_workspace_to_lines_for_test(&app, 100, 40);
    let all = lines.join("\n");

    assert!(
        !all.contains("Ask Spirit"),
        "Git view footer must not show the composer 'Ask Spirit';\nlines:\n{}",
        all
    );
    assert!(
        all.contains("[1]") || all.contains("Chat") && all.contains("[Tab]"),
        "Git view footer must show keyboard hints;\nlines:\n{}",
        all
    );
}

#[test]
fn sessions_view_footer_shows_workspace_hints() {
    let mut app = WorkspaceApp::new_for_test(REPO_ROOT);
    app.select_view(WorkspaceView::Sessions);
    app.set_session_summary_for_test(SessionSummary::default());

    let lines = render_workspace_to_lines_for_test(&app, 100, 40);
    let all = lines.join("\n");

    assert!(
        !all.contains("Ask Spirit"),
        "Sessions view footer must not show the composer 'Ask Spirit';\nlines:\n{}",
        all
    );
}
