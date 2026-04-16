use blazar::welcome::startup::{WelcomeController, run_session};
use std::io::{self, BufRead, Read};

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

#[test]
fn run_session_shows_idle_before_listening() {
    let mut input = io::Cursor::new("status report\n");
    let mut output = Vec::new();

    run_session(&mut input, &mut output).expect("session render should succeed");

    let transcript = String::from_utf8(output).expect("session output should be utf-8");
    let greeting = transcript
        .find("A rainbow helper just spotted you")
        .expect("session should start with greeting");
    let idle = transcript
        .find("Waiting with a sprinkle of stardust")
        .expect("session should show the idle waiting frame");
    let listening = transcript
        .find("Listening with twinkly focus")
        .expect("session should end in the listening frame");

    assert!(greeting < idle);
    assert!(idle < listening);
}

#[test]
fn run_session_keeps_idle_frame_on_eof() {
    let mut input = io::Cursor::new("");
    let mut output = Vec::new();

    run_session(&mut input, &mut output).expect("session render should succeed");

    let transcript = String::from_utf8(output).expect("session output should be utf-8");
    assert!(transcript.contains("A rainbow helper just spotted you"));
    assert!(transcript.contains("Waiting with a sprinkle of stardust"));
    assert!(!transcript.contains("Listening with twinkly focus"));
}

#[test]
fn run_session_propagates_input_errors() {
    let mut input = BrokenReader;
    let mut output = Vec::new();

    let error = run_session(&mut input, &mut output).expect_err("read failure should bubble up");
    assert_eq!(error.kind(), io::ErrorKind::Other);
}

struct BrokenReader;

impl Read for BrokenReader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("broken reader"))
    }
}

impl BufRead for BrokenReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Err(io::Error::other("broken reader"))
    }

    fn consume(&mut self, _amt: usize) {}
}
