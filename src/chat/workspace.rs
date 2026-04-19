use crate::chat::app::ChatApp;
use crate::chat::git::GitSummary;
use crate::chat::input::InputAction;
use crate::chat::session::SessionSummary;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceView {
    Chat,
    Git,
    Sessions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceFocus {
    Nav,
    Content,
    Footer,
}

pub struct WorkspaceApp {
    chat: ChatApp,
    active_view: WorkspaceView,
    focus: WorkspaceFocus,
    git_summary: GitSummary,
    session_summary: SessionSummary,
}

impl WorkspaceApp {
    /// Creates a `WorkspaceApp` that loads live Git and session data.
    pub fn new(repo_path: &str) -> Self {
        let path = if repo_path.is_empty() {
            std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf())
        } else {
            Path::new(repo_path).to_path_buf()
        };
        Self {
            chat: ChatApp::new(repo_path),
            active_view: WorkspaceView::Chat,
            focus: WorkspaceFocus::Nav,
            git_summary: GitSummary::load(&path),
            session_summary: SessionSummary::load(&path),
        }
    }

    pub fn new_with_view(repo_path: &str, view: WorkspaceView) -> Self {
        let mut app = Self::new(repo_path);
        app.select_view(view);
        app
    }

    /// Creates a `WorkspaceApp` with deterministic test fixtures.
    ///
    /// Does **not** invoke live Git or session loaders so tests remain
    /// independent of the caller's real repository / session state.
    pub fn new_for_test(_repo_path: &str) -> Self {
        Self {
            chat: ChatApp::new(_repo_path),
            active_view: WorkspaceView::Chat,
            focus: WorkspaceFocus::Nav,
            git_summary: GitSummary::for_test(),
            session_summary: SessionSummary::for_test(),
        }
    }

    pub fn should_quit(&self) -> bool {
        self.chat.should_quit()
    }

    pub fn git_summary(&self) -> &GitSummary {
        &self.git_summary
    }

    pub fn set_git_summary_for_test(&mut self, summary: GitSummary) {
        self.git_summary = summary;
    }

    pub fn session_summary(&self) -> &SessionSummary {
        &self.session_summary
    }

    pub fn set_session_summary_for_test(&mut self, summary: SessionSummary) {
        self.session_summary = summary;
    }

    pub fn active_view(&self) -> WorkspaceView {
        self.active_view
    }

    pub fn focus(&self) -> WorkspaceFocus {
        self.focus
    }

    pub fn select_view(&mut self, view: WorkspaceView) {
        self.active_view = view;
        self.focus = WorkspaceFocus::Content;
    }

    pub fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            WorkspaceFocus::Nav => WorkspaceFocus::Content,
            WorkspaceFocus::Content => WorkspaceFocus::Footer,
            WorkspaceFocus::Footer => WorkspaceFocus::Nav,
        };
    }

    pub fn handle_action(&mut self, action: InputAction) {
        // Quit must be handled unconditionally regardless of focus or view so
        // that Esc / Ctrl-C always works even from a non-Chat footer.
        if action == InputAction::Quit {
            self.chat_mut().handle_action(action);
            return;
        }
        if action == InputAction::CycleFocus {
            self.cycle_focus();
        } else if self.focus == WorkspaceFocus::Footer
            && self.active_view == WorkspaceView::Chat
            && matches!(
                action,
                InputAction::SelectChatView
                    | InputAction::SelectGitView
                    | InputAction::SelectSessionsView
            )
        {
            // Chat view footer acts as the live composer: re-encode digit shortcuts as key events
            let ch = match action {
                InputAction::SelectChatView => '1',
                InputAction::SelectGitView => '2',
                _ => '3',
            };
            self.chat_mut()
                .handle_action(InputAction::Key(KeyEvent::new(
                    KeyCode::Char(ch),
                    KeyModifiers::NONE,
                )));
        } else if action == InputAction::SelectChatView {
            self.select_view(WorkspaceView::Chat);
        } else if action == InputAction::SelectGitView {
            self.select_view(WorkspaceView::Git);
        } else if action == InputAction::SelectSessionsView {
            self.select_view(WorkspaceView::Sessions);
        } else if self.focus == WorkspaceFocus::Footer && self.active_view != WorkspaceView::Chat {
            // Non-chat footer shows only hints; discard all remaining input so it
            // never reaches the hidden composer.
        } else {
            self.chat_mut().handle_action(action);
        }
    }

    pub fn chat(&self) -> &ChatApp {
        &self.chat
    }

    pub fn chat_mut(&mut self) -> &mut ChatApp {
        &mut self.chat
    }
}
