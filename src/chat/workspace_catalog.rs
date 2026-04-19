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

/// Returns the persisted workspace catalog path.
///
/// Uses `$HOME/.copilot/blazar/workspaces.json` when `HOME` is set to an
/// absolute path. Otherwise it falls back to an explicit absolute path under
/// the current working directory, then the current executable directory:
/// `.copilot-home/blazar/workspaces.json`.
pub fn workspace_catalog_path() -> PathBuf {
    let executable_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));

    workspace_catalog_path_from_home(
        std::env::var_os("HOME").map(PathBuf::from),
        std::env::current_dir().ok().as_deref(),
        executable_dir.as_deref(),
    )
}

fn workspace_catalog_path_from_home(
    home: Option<PathBuf>,
    current_dir: Option<&Path>,
    executable_dir: Option<&Path>,
) -> PathBuf {
    if let Some(home) = home.filter(|path| path.is_absolute()) {
        return home.join(".copilot").join("blazar").join("workspaces.json");
    }

    current_dir
        .filter(|path| path.is_absolute())
        .or(executable_dir.filter(|path| path.is_absolute()))
        .map(|path| {
            path.join(".copilot-home")
                .join("blazar")
                .join("workspaces.json")
        })
        .unwrap_or_else(|| {
            PathBuf::from("/")
                .join(".copilot-home")
                .join("blazar")
                .join("workspaces.json")
        })
}

impl WorkspaceCatalog {
    /// Chooses the initial app launch target.
    ///
    /// Precedence is fixed:
    /// 1. `force_launcher` always shows the launcher.
    /// 2. A valid `last_opened` path wins over `repo_path_hint`.
    /// 3. A valid `repo_path_hint` is used only when `last_opened` is absent or invalid.
    pub fn decide_startup(&self, preference: StartupPreference) -> LaunchDecision {
        if preference.force_launcher {
            return LaunchDecision::ShowLauncher;
        }

        if let Some(last) = &self.last_opened {
            let path = PathBuf::from(last);
            if path.exists() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_catalog_path_uses_explicit_absolute_fallback_when_home_is_unavailable() {
        let path =
            workspace_catalog_path_from_home(None, Some(Path::new("/workspace")), None);

        assert_eq!(
            path,
            PathBuf::from("/workspace")
                .join(".copilot-home")
                .join("blazar")
                .join("workspaces.json")
        );
    }

    #[test]
    fn workspace_catalog_path_uses_executable_dir_when_home_and_cwd_are_unavailable() {
        let path = workspace_catalog_path_from_home(None, None, Some(Path::new("/app/bin")));

        assert_eq!(
            path,
            PathBuf::from("/app/bin")
                .join(".copilot-home")
                .join("blazar")
                .join("workspaces.json")
        );
    }

    #[test]
    fn workspace_catalog_path_uses_absolute_executable_dir_when_current_dir_is_relative() {
        let path = workspace_catalog_path_from_home(
            None,
            Some(Path::new("relative-workspace")),
            Some(Path::new("/app/bin")),
        );

        assert_eq!(
            path,
            PathBuf::from("/app/bin")
                .join(".copilot-home")
                .join("blazar")
                .join("workspaces.json")
        );
    }
}
