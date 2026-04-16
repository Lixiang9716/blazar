use blazar::welcome::startup::WelcomeController;
use std::io;

fn main() {
    let mut welcome = WelcomeController::new();
    println!("{}", welcome.frame(0, ""));

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("stdin read should succeed");

    println!("{}", welcome.frame(1_500, &input));
}
