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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsersLayoutPolicy {
    pub top_height: u16,
    pub input_height: u16,
    pub model_height: u16,
    pub max_command_window_size: u16,
}

impl Default for UsersLayoutPolicy {
    fn default() -> Self {
        Self {
            top_height: 1,
            input_height: 1,
            model_height: 1,
            max_command_window_size: 6,
        }
    }
}

impl UsersLayoutPolicy {
    pub const fn total_panel_height(self) -> u16 {
        self.top_height
            .saturating_add(self.input_height)
            .saturating_add(self.model_height)
    }

    pub const fn total_height(self) -> u16 {
        self.total_panel_height().saturating_add(1)
    }

    pub fn users_area_height(self, total_height: u16) -> u16 {
        if total_height <= 1 {
            0
        } else {
            self.total_height().min(total_height.saturating_sub(1))
        }
    }
}
