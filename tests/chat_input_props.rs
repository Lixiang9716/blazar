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
    let action = InputAction::from_key_event(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    ));

    assert_eq!(action, InputAction::Submit);
}
