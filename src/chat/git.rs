use std::path::Path;

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
    /// Loads live Git state from `repo_path`.
    ///
    /// If the path is not a Git repository or git is unavailable, returns a
    /// clearly non-real fallback (branch = `"(git unavailable)"`) rather than
    /// silently returning `"HEAD"`.
    pub fn load(repo_path: &Path) -> Self {
        let path_str = repo_path.to_str().unwrap_or(".");

        let status_out = std::process::Command::new("git")
            .args(["-C", path_str, "status", "--short", "--branch"])
            .output();

        let Ok(status_out) = status_out else {
            return Self::unavailable();
        };
        if !status_out.status.success() {
            return Self::unavailable();
        }

        let text = String::from_utf8_lossy(&status_out.stdout);
        let mut branch = String::from("(unknown branch)");
        let mut ahead = 0usize;
        let mut behind = 0usize;
        let mut staged = 0usize;
        let mut unstaged = 0usize;
        let mut changed_files = Vec::new();
        let mut is_dirty = false;

        for line in text.lines() {
            if line.starts_with("## ") {
                let rest = &line[3..];
                // Extract branch name (before "...")
                branch = if let Some(pos) = rest.find("...") {
                    rest[..pos].to_string()
                } else {
                    rest.split_whitespace()
                        .next()
                        .unwrap_or("(unknown branch)")
                        .to_string()
                };
                // Parse [ahead N, behind M]
                if let Some(ab_start) = rest.find('[') {
                    let end = rest.find(']').unwrap_or(rest.len());
                    let ab_str = &rest[ab_start + 1..end];
                    for part in ab_str.split(',') {
                        let part = part.trim();
                        if let Some(n) = part.strip_prefix("ahead ") {
                            ahead = n.trim().parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix("behind ") {
                            behind = n.trim().parse().unwrap_or(0);
                        }
                    }
                }
            } else if line.len() >= 3 {
                let x = &line[..1]; // index status
                let y = &line[1..2]; // worktree status
                let file = line[3..].to_string();
                // Untracked files (XY == "??") count as dirty/changed but not staged/unstaged
                if x == "?" && y == "?" {
                    is_dirty = true;
                    changed_files.push(file);
                } else {
                    if x != " " {
                        staged += 1;
                    }
                    if y != " " {
                        unstaged += 1;
                    }
                    is_dirty = true;
                    changed_files.push(file);
                }
            }
        }

        // Recent commits
        let log_out = std::process::Command::new("git")
            .args([
                "-C",
                path_str,
                "log",
                "--oneline",
                "-n",
                "5",
                "--no-decorate",
            ])
            .output();

        let recent_commits = if let Ok(out) = log_out {
            if out.status.success() {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        Self {
            branch,
            is_dirty,
            ahead,
            behind,
            staged,
            unstaged,
            changed_files,
            recent_commits,
        }
    }

    fn unavailable() -> Self {
        Self {
            branch: "(git unavailable)".to_string(),
            ..Self::default()
        }
    }

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
