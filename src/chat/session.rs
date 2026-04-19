use std::path::Path;

/// A lightweight, read-oriented snapshot of the current session state.
#[derive(Debug, Clone, Default)]
pub struct SessionSummary {
    pub session_label: String,
    pub cwd: String,
    pub active_intent: String,
    pub plan_status: String,
    pub checkpoints: Vec<String>,
    pub ready_todos: usize,
    pub in_progress_todos: usize,
    pub done_todos: usize,
}

impl SessionSummary {
    /// Loads live session state.
    ///
    /// Derives the session directory from the `COPILOT_AGENT_SESSION_ID`
    /// environment variable (`~/.copilot/session-state/<id>`).  If the env
    /// var is absent or the directory does not exist, returns an intentionally
    /// empty summary rather than bogus placeholder data.
    pub fn load(repo_path: &Path) -> Self {
        let session_dir = std::env::var("COPILOT_AGENT_SESSION_ID").ok().map(|id| {
            let home = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(home)
                .join(".copilot/session-state")
                .join(id)
        });
        Self::load_from_dir(repo_path, session_dir.as_deref())
    }

    /// Loads live session state from a specific `session_dir`.
    ///
    /// Exposed as `pub` so tests can supply a deterministic fixture directory
    /// without setting environment variables.
    pub fn load_from_dir(repo_path: &Path, session_dir: Option<&Path>) -> Self {
        let Some(dir) = session_dir else {
            return Self::default();
        };
        if !dir.exists() {
            return Self::default();
        }

        // Session label: read from workspace.yaml or fall back to dir name
        let session_label = read_yaml_field(dir.join("workspace.yaml").as_path(), "label")
            .or_else(|| {
                dir.file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_owned)
            })
            .unwrap_or_default();

        // CWD from repo_path (or workspace.yaml if repo_path is just ".")
        let cwd = if repo_path == Path::new(".") {
            read_yaml_field(dir.join("workspace.yaml").as_path(), "repoPath")
                .unwrap_or_else(|| ".".to_string())
        } else {
            repo_path.display().to_string()
        };

        // Plan status
        let plan_status = if dir.join("plan.md").exists() {
            "plan.md present".to_string()
        } else {
            "No plan".to_string()
        };

        // Checkpoints from checkpoints/index.md
        let checkpoints = load_checkpoints(dir.join("checkpoints/index.md").as_path());

        // Todo counts from session.db
        let (ready_todos, in_progress_todos, done_todos) =
            load_todo_counts(dir.join("session.db").as_path());

        Self {
            session_label,
            cwd,
            active_intent: "No active intent recorded".to_string(),
            plan_status,
            checkpoints,
            ready_todos,
            in_progress_todos,
            done_todos,
        }
    }

    /// Returns a deterministic seed suitable for tests.
    pub fn for_test() -> Self {
        Self {
            session_label: "spirit-workspace-tui".to_string(),
            cwd: "/home/lx/blazar".to_string(),
            active_intent: "Implementing Sessions workspace view".to_string(),
            plan_status: "plan.md · 6 tasks · 4 done".to_string(),
            checkpoints: vec![
                "Checkpoint 004".to_string(),
                "Checkpoint 008".to_string(),
                "Checkpoint 009".to_string(),
            ],
            ready_todos: 2,
            in_progress_todos: 1,
            done_todos: 4,
        }
    }
}

/// Reads a top-level `key: value` field from a minimal YAML file.
/// Only handles simple scalar values on the same line as the key.
fn read_yaml_field(path: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let prefix = format!("{key}:");
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(&prefix) {
            let value = rest.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Extracts checkpoint names from `checkpoints/index.md`.
/// Collects lines containing "Checkpoint" (case-sensitive) as entries.
fn load_checkpoints(index_path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(index_path) else {
        return vec![];
    };
    text.lines()
        .filter(|l| l.contains("Checkpoint"))
        .map(|l| {
            // Strip leading list markers ("- ", "* ") and trim
            let trimmed = l.trim_start_matches([' ', '-', '*']).trim();
            // Keep up to the first colon to avoid very long lines
            trimmed
                .split_once(':')
                .map_or(trimmed, |(left, _)| left.trim())
                .to_string()
        })
        .collect()
}

/// Queries `session.db` for todo counts grouped by status.
/// Returns `(pending, in_progress, done)`.
fn load_todo_counts(db_path: &Path) -> (usize, usize, usize) {
    let Ok(conn) = rusqlite::Connection::open(db_path) else {
        return (0, 0, 0);
    };
    let mut pending = 0usize;
    let mut in_progress = 0usize;
    let mut done = 0usize;

    let query = "SELECT status, COUNT(*) FROM todos GROUP BY status";
    let Ok(mut stmt) = conn.prepare(query) else {
        return (0, 0, 0);
    };
    let Ok(rows) = stmt.query_map([], |row| {
        let status: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        Ok((status, count as usize))
    }) else {
        return (0, 0, 0);
    };

    for row in rows.flatten() {
        match row.0.as_str() {
            "pending" => pending = row.1,
            "in_progress" => in_progress = row.1,
            "done" => done = row.1,
            _ => {}
        }
    }

    (pending, in_progress, done)
}
