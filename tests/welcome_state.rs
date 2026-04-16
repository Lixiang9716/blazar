use blazar::welcome::state::{PresenceMode, WelcomeState};

#[test]
fn mascot_starts_in_greeting_mode() {
    assert_eq!(WelcomeState::new().mode(), PresenceMode::Greeting);
}

#[test]
fn mascot_settles_into_idle_sparkle_after_first_beat() {
    let state = WelcomeState::new().tick(1_200, false);
    assert_eq!(state.mode(), PresenceMode::IdleSparkle);
}

#[test]
fn typing_input_switches_the_mascot_to_listening() {
    let state = WelcomeState::new().tick(100, true);
    assert_eq!(state.mode(), PresenceMode::Listening);
}

#[test]
fn repeated_input_resets_the_listening_timer() {
    let state = WelcomeState::new().tick(100, true);
    let state = state.tick(1_200, true);
    let state = state.tick(2_400, false);
    assert_eq!(state.mode(), PresenceMode::Listening);

    let state = state.tick(2_801, false);
    assert_eq!(state.mode(), PresenceMode::IdleSparkle);
}
