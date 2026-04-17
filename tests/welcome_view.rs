use blazar::welcome::state::WelcomeState;
use blazar::welcome::view::render_scene;

#[test]
fn welcome_scene_contains_brand_copy_and_prompt() {
    let scene = render_scene(WelcomeState::new(), 0);

    assert!(scene.contains("BLAZAR"));
    assert!(scene.contains("Star Sugar Guidepony / 星糖导航马"));
    assert!(scene.contains("A rainbow helper just spotted you"));
    assert!(scene.contains("Describe a task to begin"));
}

#[test]
fn welcome_scene_keeps_sprite_and_copy_columns_together() {
    let scene = render_scene(WelcomeState::new(), 0);

    assert!(scene.lines().count() >= 6);
    assert!(scene.contains("> "));
}
