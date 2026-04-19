// Tests for GitSummary::load() — uses a real temp git repo as a fixture.
use blazar::chat::git::GitSummary;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nanos}"))
}

fn init_git_repo(dir: &PathBuf) {
    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(dir)
        .output()
        .expect("git init failed");
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .output()
        .unwrap();
}

fn commit_all(dir: &PathBuf, msg: &str) {
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", msg])
        .current_dir(dir)
        .output()
        .unwrap();
}

#[test]
fn git_loader_reads_branch_name() {
    let dir = unique_dir("blazar-git-branch");
    std::fs::create_dir_all(&dir).unwrap();
    init_git_repo(&dir);
    std::fs::write(dir.join("README.md"), "hello").unwrap();
    commit_all(&dir, "feat: initial commit");

    let summary = GitSummary::load(&dir);

    assert_eq!(summary.branch, "main", "branch should be 'main'");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn git_loader_reads_recent_commits() {
    let dir = unique_dir("blazar-git-commits");
    std::fs::create_dir_all(&dir).unwrap();
    init_git_repo(&dir);
    std::fs::write(dir.join("a.txt"), "a").unwrap();
    commit_all(&dir, "feat: first commit");
    std::fs::write(dir.join("b.txt"), "b").unwrap();
    commit_all(&dir, "chore: second commit");

    let summary = GitSummary::load(&dir);

    assert!(
        summary
            .recent_commits
            .iter()
            .any(|c| c.contains("first commit")),
        "should include 'first commit', got: {:?}",
        summary.recent_commits
    );
    assert!(
        summary
            .recent_commits
            .iter()
            .any(|c| c.contains("second commit")),
        "should include 'second commit'"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn git_loader_detects_unstaged_changes() {
    let dir = unique_dir("blazar-git-dirty");
    std::fs::create_dir_all(&dir).unwrap();
    init_git_repo(&dir);
    std::fs::write(dir.join("README.md"), "original").unwrap();
    commit_all(&dir, "chore: initial");

    // Modify after commit → unstaged change
    std::fs::write(dir.join("README.md"), "modified").unwrap();

    let summary = GitSummary::load(&dir);

    assert!(
        summary.is_dirty,
        "repo should be dirty after uncommitted change"
    );
    assert!(
        summary
            .changed_files
            .iter()
            .any(|f| f.contains("README.md")),
        "should list README.md as changed, got: {:?}",
        summary.changed_files
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn git_loader_clean_repo_is_not_dirty() {
    let dir = unique_dir("blazar-git-clean");
    std::fs::create_dir_all(&dir).unwrap();
    init_git_repo(&dir);
    std::fs::write(dir.join("README.md"), "hello").unwrap();
    commit_all(&dir, "chore: initial");

    let summary = GitSummary::load(&dir);

    assert!(!summary.is_dirty, "clean repo should not be dirty");
    assert!(
        summary.changed_files.is_empty(),
        "clean repo should have no changed files"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn git_loader_invalid_path_returns_non_default_fallback() {
    let dir = PathBuf::from("/nonexistent/path/that/does/not/exist");
    let summary = GitSummary::load(&dir);
    // Must not silently pretend to be "HEAD" with no indication of failure
    assert!(
        summary.branch.contains("unavailable")
            || summary.branch.contains("unknown")
            || summary.branch.contains("error"),
        "fallback branch should indicate failure, got: {}",
        summary.branch
    );
}
