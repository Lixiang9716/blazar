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

#[test]
fn typing_focus_times_out_to_idle_monitor() {
    let state = WelcomeState::new().tick(200, true);
    let state = state.tick(1_300, false);
    assert_eq!(state.mode(), PresenceMode::IdleMonitor);
}

#[test]
fn idle_monitor_times_out_to_on_watch() {
    let state = WelcomeState::new().tick(600, false);
    let state = state.tick(1_400, false);
    assert_eq!(state.mode(), PresenceMode::IdleMonitor);

    let state = state.tick(6_400, false);
    assert_eq!(state.mode(), PresenceMode::OnWatch);
}

#[test]
fn continuous_typing_resets_typing_timer() {
    let state = WelcomeState::new().tick(200, true); // entered_at 200
    let state = state.tick(800, true); // resets to 800
    assert_eq!(state.mode(), PresenceMode::TypingFocus);

    let state = state.tick(1_600, false); // elapsed from 800 = 800 < 1_000
    assert_eq!(state.mode(), PresenceMode::TypingFocus);

    let state = state.tick(1_900, false); // elapsed from 800 = 1_100 > 1_000
    assert_eq!(state.mode(), PresenceMode::IdleMonitor);
}
