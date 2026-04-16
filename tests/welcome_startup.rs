use blazar::welcome::startup::WelcomeController;

#[test]
fn controller_starts_on_watch_then_acknowledges_user() {
    let mut controller = WelcomeController::new();

    let first = controller.frame(0, "");
    assert!(first.contains("Calibrating star map"));

    let second = controller.frame(600, "");
    assert!(second.contains("Turning toward your terminal"));
}

#[test]
fn controller_focuses_when_input_buffer_is_present() {
    let mut controller = WelcomeController::new();
    let frame = controller.frame(1_200, "build a parser");

    assert!(frame.contains("Listening for your request"));
}
