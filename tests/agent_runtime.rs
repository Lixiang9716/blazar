use blazar::agent::runtime::AgentRuntime;
use blazar::provider::echo::EchoProvider;
use blazar::provider::{LlmProvider, ProviderEvent};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn echo_provider_streams_full_response() {
    let provider = EchoProvider::new(0); // no delay for test speed
    let (tx, rx) = mpsc::channel();

    provider.stream_turn("hi", tx);

    let mut text = String::new();
    let mut completed = false;
    for event in rx {
        match event {
            ProviderEvent::TextDelta(chunk) => text.push_str(&chunk),
            ProviderEvent::TurnComplete => {
                completed = true;
                break;
            }
            ProviderEvent::Error(_) => panic!("unexpected error"),
            ProviderEvent::ToolCallRequest(_) => {}
        }
    }

    assert!(completed);
    assert_eq!(text, "Echo: hi");
}

#[test]
fn runtime_round_trip() {
    let runtime = AgentRuntime::new(Box::new(EchoProvider::new(0)));

    runtime.submit_turn("hello");

    // Collect events with a timeout.
    let mut events = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(event) = runtime.try_recv() {
            events.push(event.clone());
            if matches!(
                event,
                blazar::agent::protocol::AgentEvent::TurnComplete
                    | blazar::agent::protocol::AgentEvent::TurnFailed { .. }
            ) {
                break;
            }
        }
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for agent events");
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    // First event should be TurnStarted
    assert!(matches!(
        &events[0],
        blazar::agent::protocol::AgentEvent::TurnStarted { .. }
    ));

    // Last event should be TurnComplete
    assert!(matches!(
        events.last().unwrap(),
        blazar::agent::protocol::AgentEvent::TurnComplete
    ));

    // Collect all text deltas
    let text: String = events
        .iter()
        .filter_map(|e| match e {
            blazar::agent::protocol::AgentEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "Echo: hello");
}
