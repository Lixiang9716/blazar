use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use super::protocol::{AgentCommand, AgentEvent};
use crate::provider::{LlmProvider, ProviderEvent};

/// The agent runtime bridges the synchronous TUI render loop and
/// the (potentially blocking) LLM provider.
///
/// It spawns a background thread that:
/// 1. Waits for `AgentCommand`s from the UI.
/// 2. Runs the provider in a scoped sub-thread for real-time streaming.
/// 3. Relays `AgentEvent`s back to the UI via a channel.
///
/// The UI calls `try_recv()` each tick to drain available events.
pub struct AgentRuntime {
    cmd_tx: Sender<AgentCommand>,
    event_rx: Receiver<AgentEvent>,
    _handle: JoinHandle<()>,
}

impl AgentRuntime {
    /// Create a new runtime with the given provider.
    pub fn new(provider: Box<dyn LlmProvider>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        let handle = std::thread::Builder::new()
            .name("blazar-agent".into())
            .spawn(move || runtime_loop(cmd_rx, event_tx, provider))
            .expect("failed to spawn agent runtime thread");

        Self {
            cmd_tx,
            event_rx,
            _handle: handle,
        }
    }

    /// Submit a new turn to the agent.
    pub fn submit_turn(&self, prompt: &str) {
        let _ = self.cmd_tx.send(AgentCommand::SubmitTurn {
            prompt: prompt.to_string(),
        });
    }

    /// Non-blocking poll for the next event. Returns `None` if no event
    /// is available. Call this in the render-loop tick.
    pub fn try_recv(&self) -> Option<AgentEvent> {
        self.event_rx.try_recv().ok()
    }
}

impl Drop for AgentRuntime {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(AgentCommand::Shutdown);
    }
}

/// The main loop running on the background thread.
fn runtime_loop(
    cmd_rx: Receiver<AgentCommand>,
    event_tx: Sender<AgentEvent>,
    provider: Box<dyn LlmProvider>,
) {
    let mut turn_counter = 0u64;

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            AgentCommand::SubmitTurn { prompt } => {
                turn_counter += 1;
                let turn_id = format!("turn-{turn_counter}");

                if event_tx
                    .send(AgentEvent::TurnStarted {
                        turn_id: turn_id.clone(),
                    })
                    .is_err()
                {
                    break;
                }

                run_turn(&prompt, &*provider, &event_tx);
            }
            AgentCommand::Cancel => {
                // Future: signal the provider sub-thread to stop.
            }
            AgentCommand::Shutdown => break,
        }
    }
}

/// Execute a single ReAct turn: stream provider output and relay events.
///
/// Uses `std::thread::scope` so the provider runs in a sub-thread while
/// the current thread relays events to the UI in real time.
fn run_turn(prompt: &str, provider: &dyn LlmProvider, event_tx: &Sender<AgentEvent>) {
    let (chunk_tx, chunk_rx) = mpsc::channel::<ProviderEvent>();

    std::thread::scope(|s| {
        // Provider streams chunks in a scoped sub-thread.
        s.spawn(|| {
            provider.stream_turn(prompt, chunk_tx);
        });

        // Relay provider events → agent events in real time.
        let mut got_terminal = false;
        for prov_event in &chunk_rx {
            let agent_event = match prov_event {
                ProviderEvent::TextDelta(text) => AgentEvent::TextDelta { text },
                ProviderEvent::ToolCallRequest(payload) => AgentEvent::ToolCallRequest { payload },
                ProviderEvent::TurnComplete => {
                    got_terminal = true;
                    AgentEvent::TurnComplete
                }
                ProviderEvent::Error(err) => {
                    got_terminal = true;
                    AgentEvent::TurnFailed { error: err }
                }
            };
            if event_tx.send(agent_event).is_err() {
                break;
            }
        }

        // Safety net: if provider didn't send a terminal event, emit one.
        if !got_terminal {
            let _ = event_tx.send(AgentEvent::TurnComplete);
        }
    });
}
