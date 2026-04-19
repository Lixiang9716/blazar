use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq)]
pub enum InputAction {
    Quit,
    Submit,
    CycleFocus,
    SelectChatView,
    SelectGitView,
    SelectSessionsView,
    Key(KeyEvent),
}

impl InputAction {
    pub fn from_key_event(key: KeyEvent) -> Self {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => InputAction::Quit,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => InputAction::Quit,
            (KeyCode::Enter, _) => InputAction::Submit,
            (KeyCode::Tab, _) => InputAction::CycleFocus,
            (KeyCode::Char('1'), KeyModifiers::NONE) => InputAction::SelectChatView,
            (KeyCode::Char('2'), KeyModifiers::NONE) => InputAction::SelectGitView,
            (KeyCode::Char('3'), KeyModifiers::NONE) => InputAction::SelectSessionsView,
            _ => InputAction::Key(key),
        }
    }
}
