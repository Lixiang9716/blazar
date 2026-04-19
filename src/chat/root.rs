use crate::chat::input::InputAction;
use crate::chat::launcher::{LauncherApp, LauncherOutcome};
use crate::chat::workspace::{WorkspaceApp, WorkspaceView};
use crate::chat::workspace_catalog::{LaunchDecision, WorkspaceCatalog};
use std::path::PathBuf;

pub enum RootMode {
    Launcher(LauncherApp),
    Workspace(Box<WorkspaceApp>),
}

pub struct RootApp {
    mode: RootMode,
    opened_workspace: Option<PathBuf>,
    should_quit: bool,
}

impl RootApp {
    pub fn from_launch_decision(catalog: WorkspaceCatalog, decision: LaunchDecision) -> Self {
        let mode = match decision {
            LaunchDecision::ShowLauncher => {
                RootMode::Launcher(LauncherApp::new(catalog.workspaces))
            }
            LaunchDecision::Resume {
                repo_path,
                initial_view,
            } => RootMode::Workspace(Box::new(WorkspaceApp::new_with_view(
                repo_path.to_string_lossy().as_ref(),
                initial_view.unwrap_or(WorkspaceView::Chat),
            ))),
        };

        Self {
            mode,
            opened_workspace: None,
            should_quit: false,
        }
    }

    pub fn mode(&self) -> &RootMode {
        &self.mode
    }

    pub fn workspace(&self) -> Option<&WorkspaceApp> {
        match &self.mode {
            RootMode::Workspace(workspace) => Some(workspace.as_ref()),
            RootMode::Launcher(_) => None,
        }
    }

    pub fn handle_action(&mut self, action: InputAction) -> Option<PathBuf> {
        self.opened_workspace = None;

        match &mut self.mode {
            RootMode::Launcher(_) if action == InputAction::Quit => {
                self.should_quit = true;
                None
            }
            RootMode::Launcher(launcher) => match launcher.handle_action(action) {
                LauncherOutcome::None => None,
                LauncherOutcome::OpenWorkspace {
                    repo_path,
                    initial_view,
                } => {
                    self.mode = RootMode::Workspace(Box::new(WorkspaceApp::new_with_view(
                        repo_path.to_string_lossy().as_ref(),
                        initial_view.unwrap_or(WorkspaceView::Chat),
                    )));
                    self.opened_workspace = Some(repo_path);
                    None
                }
            },
            RootMode::Workspace(workspace) => {
                workspace.handle_action(action);
                None
            }
        }
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
            || matches!(
                &self.mode,
                RootMode::Workspace(workspace) if workspace.should_quit()
            )
    }

    pub fn take_opened_workspace(&mut self) -> Option<PathBuf> {
        self.opened_workspace.take()
    }
}
