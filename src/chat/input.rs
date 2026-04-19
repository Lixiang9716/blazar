use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq)]
pub enum InputAction {
    Quit,
    Submit,
    Key(KeyEvent),
}

impl InputAction {
    #[allow(non_upper_case_globals)]
    pub const CycleFocus: Self = Self::Key(KeyEvent::new(KeyCode::F(13), KeyModifiers::NONE));
    #[allow(non_upper_case_globals)]
    pub const SelectChatView: Self = Self::Key(KeyEvent::new(KeyCode::F(14), KeyModifiers::NONE));
    #[allow(non_upper_case_globals)]
    pub const SelectGitView: Self = Self::Key(KeyEvent::new(KeyCode::F(15), KeyModifiers::NONE));
    #[allow(non_upper_case_globals)]
    pub const SelectSessionsView: Self =
        Self::Key(KeyEvent::new(KeyCode::F(16), KeyModifiers::NONE));

    pub fn from_key_event(key: KeyEvent) -> Self {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => InputAction::Quit,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => InputAction::Quit,
            (KeyCode::Enter, _) => InputAction::Submit,
            (KeyCode::Tab, _) => Self::CycleFocus,
            (KeyCode::Char('1'), KeyModifiers::NONE) => Self::SelectChatView,
            (KeyCode::Char('2'), KeyModifiers::NONE) => Self::SelectGitView,
            (KeyCode::Char('3'), KeyModifiers::NONE) => Self::SelectSessionsView,
            _ => InputAction::Key(key),
        }
    }
}
