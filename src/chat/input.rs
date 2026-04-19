use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq)]
pub enum InputAction {
    Quit,
    Submit,
    ToggleDetails,
    ScrollUp,
    ScrollDown,
    PickerUp,
    PickerDown,
    Backspace,
    Key(KeyEvent),
}

impl InputAction {
    pub fn from_key_event(key: KeyEvent) -> Self {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => InputAction::Quit,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => InputAction::Quit,
            (KeyCode::Char('o'), KeyModifiers::CONTROL) => InputAction::ToggleDetails,
            (KeyCode::Enter, _) => InputAction::Submit,
            (KeyCode::Up, _) => InputAction::PickerUp,
            (KeyCode::Down, _) => InputAction::PickerDown,
            (KeyCode::Backspace, _) => InputAction::Backspace,
            (KeyCode::PageUp, _) | (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                InputAction::ScrollUp
            }
            (KeyCode::PageDown, _) | (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                InputAction::ScrollDown
            }
            _ => InputAction::Key(key),
        }
    }
}
