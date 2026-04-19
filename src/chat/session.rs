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
