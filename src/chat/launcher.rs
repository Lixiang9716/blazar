use crate::chat::input::InputAction;
use crate::chat::workspace::WorkspaceView;
use crate::chat::workspace_catalog::WorkspaceRecord;
use crossterm::event::KeyCode;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherFocus {
    List,
    Preview,
    Actions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LauncherOutcome {
    None,
    OpenWorkspace {
        repo_path: PathBuf,
        initial_view: Option<WorkspaceView>,
    },
}

#[derive(Debug)]
pub struct LauncherApp {
    workspaces: Vec<WorkspaceRecord>,
    selected_index: usize,
    focus: LauncherFocus,
}

impl LauncherApp {
    pub fn new(workspaces: Vec<WorkspaceRecord>) -> Self {
        Self {
            workspaces,
            selected_index: 0,
            focus: LauncherFocus::List,
        }
    }

    pub fn workspaces(&self) -> &[WorkspaceRecord] {
        &self.workspaces
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn has_workspaces(&self) -> bool {
        !self.workspaces.is_empty()
    }

    /// Returns the currently selected workspace.
    ///
    /// # Panics
    ///
    /// Panics if the launcher has no workspaces. Callers must ensure at least
    /// one workspace exists before calling this accessor.
    pub fn selected_workspace(&self) -> &WorkspaceRecord {
        &self.workspaces[self.selected_index]
    }

    pub fn focus(&self) -> LauncherFocus {
        self.focus
    }

    /// Handles launcher input actions.
    ///
    /// `Submit`, `s`, and `g` require at least one workspace because they
    /// route through `open_selected()` and `selected_workspace()`.
    pub fn handle_action(&mut self, action: InputAction) -> LauncherOutcome {
        match action {
            InputAction::CycleFocus => {
                self.focus = match self.focus {
                    LauncherFocus::List => LauncherFocus::Preview,
                    LauncherFocus::Preview => LauncherFocus::Actions,
                    LauncherFocus::Actions => LauncherFocus::List,
                };
                LauncherOutcome::None
            }
            InputAction::Submit => self.open_selected(None),
            InputAction::Key(key) if key.code == KeyCode::Down => {
                self.selected_index =
                    (self.selected_index + 1).min(self.workspaces.len().saturating_sub(1));
                LauncherOutcome::None
            }
            InputAction::Key(key) if key.code == KeyCode::Up => {
                self.selected_index = self.selected_index.saturating_sub(1);
                LauncherOutcome::None
            }
            InputAction::Key(key) if key.code == KeyCode::Char('s') => {
                self.open_selected(Some(WorkspaceView::Sessions))
            }
            InputAction::Key(key) if key.code == KeyCode::Char('g') => {
                self.open_selected(Some(WorkspaceView::Git))
            }
            _ => LauncherOutcome::None,
        }
    }

    fn open_selected(&self, initial_view: Option<WorkspaceView>) -> LauncherOutcome {
        let selected = self.selected_workspace();
        LauncherOutcome::OpenWorkspace {
            repo_path: PathBuf::from(&selected.repo_path),
            initial_view,
        }
    }
}
