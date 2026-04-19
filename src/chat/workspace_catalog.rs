use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRecord {
    pub name: String,
    pub repo_path: String,
    pub branch: String,
    pub dirty: bool,
    pub last_session_label: Option<String>,
    pub last_intent: Option<String>,
    pub latest_checkpoint: Option<String>,
    pub ready_todos: usize,
    pub last_opened_at: u64,
}

impl WorkspaceRecord {
    pub fn named(name: &str, repo_path: &str) -> Self {
        Self {
            name: name.to_string(),
            repo_path: repo_path.to_string(),
            branch: "master".to_string(),
            dirty: false,
            last_session_label: None,
            last_intent: None,
            latest_checkpoint: None,
            ready_todos: 0,
            last_opened_at: 0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCatalog {
    pub last_opened: Option<String>,
    pub workspaces: Vec<WorkspaceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupPreference {
    pub repo_path_hint: Option<PathBuf>,
    pub force_launcher: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchDecision {
    Resume {
        repo_path: PathBuf,
        initial_view: Option<crate::chat::workspace::WorkspaceView>,
    },
    ShowLauncher,
}

pub fn workspace_catalog_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join(".copilot")
        .join("blazar")
        .join("workspaces.json")
}

impl WorkspaceCatalog {
    pub fn decide_startup(&self, preference: StartupPreference) -> LaunchDecision {
        if preference.force_launcher {
            return LaunchDecision::ShowLauncher;
        }

        if let Some(last) = &self.last_opened {
            let path = PathBuf::from(last);
            if Path::new(last).exists() {
                return LaunchDecision::Resume {
                    repo_path: path,
                    initial_view: None,
                };
            }
        }

        if let Some(path) = preference.repo_path_hint
            && path.exists()
        {
            return LaunchDecision::Resume {
                repo_path: path,
                initial_view: None,
            };
        }

        LaunchDecision::ShowLauncher
    }

    pub fn load_from_path(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text)
    }
}
