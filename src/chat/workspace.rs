use crate::chat::app::ChatApp;
use crate::chat::input::InputAction;

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
}

impl WorkspaceApp {
    pub fn new_for_test(repo_path: &str) -> Self {
        Self {
            chat: ChatApp::new_for_test(repo_path),
            active_view: WorkspaceView::Chat,
            focus: WorkspaceFocus::Nav,
        }
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
        if action == InputAction::CycleFocus {
            self.cycle_focus();
        } else if action == InputAction::SelectChatView {
            self.select_view(WorkspaceView::Chat);
        } else if action == InputAction::SelectGitView {
            self.select_view(WorkspaceView::Git);
        } else if action == InputAction::SelectSessionsView {
            self.select_view(WorkspaceView::Sessions);
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
