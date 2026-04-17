use blazar::welcome::mascot::render_mascot;
use blazar::welcome::state::WelcomeState;

#[test]
fn slime_idle_mascot_renders_as_ansi_multiline_sprite() {
    let mascot = render_mascot(WelcomeState::new(), 0);

    assert!(mascot.contains('\n'));
    assert!(mascot.contains("\u{1b}[38;2;"));
}

#[test]
fn slime_idle_animation_advances_with_elapsed_time() {
    let first = render_mascot(WelcomeState::new(), 0);
    let later = render_mascot(WelcomeState::new(), 260);

    assert_ne!(first, later);
}
