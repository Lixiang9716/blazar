use std::io::{self, Write};
use std::time::Instant;

use blazar::welcome::startup::WelcomeController;

fn main() -> io::Result<()> {
    let start = Instant::now();
    let mut welcome = WelcomeController::new();

    println!("{}", welcome.frame(0, ""));
    print!("\n> ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let elapsed_ms = start.elapsed().as_millis() as u64;
    println!("\n{}", welcome.frame(elapsed_ms, &input));

    Ok(())
}
