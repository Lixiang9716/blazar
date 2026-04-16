use blazar::welcome::state::WelcomeState;
use blazar::welcome::view::render_scene;

#[test]
fn welcome_scene_contains_brand_and_prompt() {
    let scene = render_scene(WelcomeState::new());

    assert!(scene.contains("BLAZAR"));
    assert!(scene.contains("Lightcore Emissary / 光核使者"));
    assert!(scene.contains("Calibrating star map"));
    assert!(scene.contains("Describe a task to begin"));
}

#[test]
fn typing_scene_switches_to_listening_copy() {
    let state = WelcomeState::new().tick(100, true);
    let scene = render_scene(state);

    assert!(scene.contains("Listening for your request"));
}
