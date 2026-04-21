use blazar::chat::input::InputAction;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn from_key_event_maps_control_and_navigation_shortcuts() {
    assert_eq!(
        InputAction::from_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        InputAction::Quit
    );
    assert_eq!(
        InputAction::from_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        InputAction::Quit
    );
    assert_eq!(
        InputAction::from_key_event(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)),
        InputAction::ToggleDetails
    );
    assert_eq!(
        InputAction::from_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        InputAction::Submit
    );
    assert_eq!(
        InputAction::from_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
        InputAction::PickerUp
    );
    assert_eq!(
        InputAction::from_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
        InputAction::PickerDown
    );
    assert_eq!(
        InputAction::from_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
        InputAction::Backspace
    );
    assert_eq!(
        InputAction::from_key_event(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)),
        InputAction::ScrollUp
    );
    assert_eq!(
        InputAction::from_key_event(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
        InputAction::ScrollUp
    );
    assert_eq!(
        InputAction::from_key_event(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
        InputAction::ScrollDown
    );
    assert_eq!(
        InputAction::from_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        InputAction::ScrollDown
    );
}

#[test]
fn from_key_event_falls_back_to_raw_key_action() {
    let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
    assert_eq!(InputAction::from_key_event(key), InputAction::Key(key));
}
