#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserMode {
    Auto,
    Plan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusMode {
    Normal,
    CommandList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextUsage {
    pub used_tokens: u32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsersStatusSnapshot {
    pub mode: UserMode,
    pub status_mode: StatusMode,
    pub current_path: String,
    pub branch: String,
    pub pr_label: Option<String>,
    pub referenced_files: Vec<String>,
    pub model_name: String,
    pub context_usage: Option<ContextUsage>,
}
