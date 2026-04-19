use blazar::chat::input::InputAction;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use proptest::prelude::*;

proptest! {
    #[test]
    fn printable_characters_are_forwarded_as_key_actions(ch in any::<char>().prop_filter(
        "printable non-control characters",
        |candidate| !candidate.is_control() && !matches!(candidate, '1' | '2' | '3')
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
fn workspace_shortcuts_map_to_workspace_actions() {
    let tab = InputAction::from_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    let one = InputAction::from_key_event(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
    let two = InputAction::from_key_event(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
    let three = InputAction::from_key_event(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));

    assert_eq!(tab, InputAction::CycleFocus);
    assert_eq!(one, InputAction::SelectChatView);
    assert_eq!(two, InputAction::SelectGitView);
    assert_eq!(three, InputAction::SelectSessionsView);
}
