/// A lightweight, read-oriented snapshot of the current Git repository state.
#[derive(Debug, Clone)]
pub struct GitSummary {
    pub branch: String,
    pub is_dirty: bool,
    pub ahead: usize,
    pub behind: usize,
    pub staged: usize,
    pub unstaged: usize,
    pub changed_files: Vec<String>,
    pub recent_commits: Vec<String>,
}

impl GitSummary {
    /// Returns a deterministic seed suitable for tests.
    pub fn for_test() -> Self {
        Self {
            branch: "main".to_string(),
            is_dirty: true,
            ahead: 2,
            behind: 0,
            staged: 1,
            unstaged: 3,
            changed_files: vec![
                "README.md".to_string(),
                "src/chat/git.rs".to_string(),
                "src/chat/view.rs".to_string(),
            ],
            recent_commits: vec![
                "feat: initial commit".to_string(),
                "chore: add workspace scaffold".to_string(),
            ],
        }
    }
}

impl Default for GitSummary {
    fn default() -> Self {
        Self {
            branch: "HEAD".to_string(),
            is_dirty: false,
            ahead: 0,
            behind: 0,
            staged: 0,
            unstaged: 0,
            changed_files: vec![],
            recent_commits: vec![],
        }
    }
}
