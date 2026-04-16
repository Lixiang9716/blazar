use blazar::welcome::startup::WelcomeController;

#[test]
fn controller_starts_with_a_greeting_then_settles_idle() {
    let mut controller = WelcomeController::new();

    let first = controller.frame(0, "");
    assert!(first.contains("A rainbow helper just spotted you"));

    let second = controller.frame(1_200, "");
    assert!(second.contains("Waiting with a sprinkle of stardust"));
}

#[test]
fn controller_switches_to_listening_when_input_arrives() {
    let mut controller = WelcomeController::new();

    let scene = controller.frame(200, "status report");
    assert!(scene.contains("Listening with twinkly focus"));
    assert!(scene.contains("Star Sugar Guidepony / 星糖导航马"));
}
