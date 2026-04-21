use blazar::chat::git::GitSummary;
use std::path::{Path, PathBuf};
use std::process::Command;

fn unique_dir(prefix: &str) -> PathBuf {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&base).expect("target dir should exist");
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    base.join(format!("{prefix}-{suffix}"))
}

fn run_git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .expect("git command should run");
    assert!(status.success(), "git {:?} should succeed", args);
}

#[test]
fn load_returns_unavailable_for_non_git_path() {
    let non_repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(
            "definitely-missing-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock should be monotonic")
                .as_nanos()
        ));
    let summary = GitSummary::load(&non_repo);

    assert_eq!(summary.branch, "(git unavailable)");
    assert!(!summary.is_dirty);
}

#[test]
fn load_parses_branch_ahead_dirty_counts_and_recent_commits() {
    let remote_dir = unique_dir("chat-git-remote");
    let repo_dir = unique_dir("chat-git-repo");
    std::fs::create_dir_all(&remote_dir).expect("create remote dir");
    std::fs::create_dir_all(&repo_dir).expect("create repo dir");

    run_git(&remote_dir, &["init", "--bare"]);
    run_git(&repo_dir, &["init"]);
    run_git(&repo_dir, &["config", "user.email", "bot@example.com"]);
    run_git(&repo_dir, &["config", "user.name", "Blazar Bot"]);

    std::fs::write(repo_dir.join("tracked.txt"), "v1\n").expect("write tracked file");
    run_git(&repo_dir, &["add", "tracked.txt"]);
    run_git(&repo_dir, &["commit", "-m", "initial"]);
    run_git(
        &repo_dir,
        &[
            "remote",
            "add",
            "origin",
            remote_dir.to_str().expect("utf-8 remote path"),
        ],
    );
    run_git(&repo_dir, &["push", "-u", "origin", "HEAD"]);

    std::fs::write(repo_dir.join("ahead.txt"), "ahead\n").expect("write ahead file");
    run_git(&repo_dir, &["add", "ahead.txt"]);
    run_git(&repo_dir, &["commit", "-m", "ahead commit"]);

    std::fs::write(repo_dir.join("tracked.txt"), "v2\n").expect("modify tracked file");
    std::fs::write(repo_dir.join("staged.txt"), "staged\n").expect("write staged file");
    run_git(&repo_dir, &["add", "staged.txt"]);
    std::fs::write(repo_dir.join("untracked.txt"), "untracked\n").expect("write untracked file");

    let summary = GitSummary::load(&repo_dir);
    assert_ne!(summary.branch, "(git unavailable)");
    assert!(summary.ahead >= 1);
    assert!(summary.is_dirty);
    assert!(summary.staged >= 1);
    assert!(summary.unstaged >= 1);
    assert!(
        summary
            .changed_files
            .iter()
            .any(|f| f.contains("tracked.txt"))
    );
    assert!(
        summary
            .changed_files
            .iter()
            .any(|f| f.contains("staged.txt"))
    );
    assert!(
        summary
            .changed_files
            .iter()
            .any(|f| f.contains("untracked.txt"))
    );
    assert!(!summary.recent_commits.is_empty());

    std::fs::remove_dir_all(repo_dir).expect("cleanup repo");
    std::fs::remove_dir_all(remote_dir).expect("cleanup remote");
}

#[test]
fn default_and_for_test_variants_are_stable() {
    let default = GitSummary::default();
    assert_eq!(default.branch, "HEAD");
    assert!(!default.is_dirty);

    let fixture = GitSummary::for_test();
    assert_eq!(fixture.branch, "main");
    assert!(fixture.is_dirty);
    assert!(!fixture.changed_files.is_empty());
}
