use blazar::welcome::state::{PresenceMode, WelcomeState};

#[test]
fn mascot_starts_on_watch() {
    assert_eq!(WelcomeState::new().mode(), PresenceMode::OnWatch);
}

#[test]
fn mascot_turns_toward_user_after_initial_beat() {
    let state = WelcomeState::new().tick(600, false);
    assert_eq!(state.mode(), PresenceMode::TurningToUser);

    let state = state.tick(1_400, false);
    assert_eq!(state.mode(), PresenceMode::IdleMonitor);
}

#[test]
fn typing_input_immediately_triggers_focus_mode() {
    let state = WelcomeState::new().tick(200, true);
    assert_eq!(state.mode(), PresenceMode::TypingFocus);
}
