use blazar::chat::input::InputAction;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use proptest::prelude::*;

proptest! {
    #[test]
    fn printable_characters_are_forwarded_as_key_actions(ch in any::<char>().prop_filter(
        "printable non-control characters",
        |candidate| !candidate.is_control()
    )) {
        let action = InputAction::from_key_event(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        ));

        prop_assert_eq!(
            action,
            InputAction::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE))
        );
    }
}

#[test]
fn enter_maps_to_submit() {
    let action = InputAction::from_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(action, InputAction::Submit);
}

#[test]
fn shift_enter_maps_to_insert_newline() {
    let action = InputAction::from_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));

    assert_eq!(action, InputAction::InsertNewline);
}

#[test]
fn backtab_maps_to_toggle_mode() {
    let action = InputAction::from_key_event(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));

    assert_eq!(action, InputAction::ToggleMode);
}

#[test]
fn tab_maps_to_regular_key_input() {
    let action = InputAction::from_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    assert_eq!(
        action,
        InputAction::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
    );
}
