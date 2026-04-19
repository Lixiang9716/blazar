use crate::chat::app::ChatApp;

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

    pub fn chat(&self) -> &ChatApp {
        &self.chat
    }

    pub fn chat_mut(&mut self) -> &mut ChatApp {
        &mut self.chat
    }
}
