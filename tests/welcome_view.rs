use blazar::welcome::state::WelcomeState;
use blazar::welcome::theme::MASCOT_PALETTE;
use blazar::welcome::view::render_scene;

#[test]
fn welcome_scene_contains_brand_copy_and_prompt() {
    let scene = render_scene(WelcomeState::new());

    assert!(scene.contains("BLAZAR"));
    assert!(scene.contains("Star Sugar Guidepony / 星糖导航马"));
    assert!(scene.contains("A rainbow helper just spotted you"));
    assert!(scene.contains("Describe a task to begin"));
}

#[test]
fn listening_scene_uses_focus_copy_and_pastel_colors() {
    let state = WelcomeState::new().tick(100, true);
    let scene = render_scene(state);

    assert!(scene.contains("Listening with twinkly focus"));
    assert!(scene.contains(MASCOT_PALETTE.pink_ansi));
}
