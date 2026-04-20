use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use log::{debug, info, warn};

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
    cancel_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

/// Maximum number of transient-error retries per turn.
const MAX_TRANSIENT_RETRIES: u32 = 1;

impl AgentRuntime {
    /// Create a new runtime with the given provider.
    pub fn new(provider: Box<dyn LlmProvider>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&cancel_flag);

        let handle = std::thread::Builder::new()
            .name("blazar-agent".into())
            .spawn(move || runtime_loop(cmd_rx, event_tx, provider, flag_clone))
            .expect("failed to spawn agent runtime thread");

        Self {
            cmd_tx,
            event_rx,
            cancel_flag,
            handle: Some(handle),
        }
    }

    /// Submit a new turn to the agent.
    ///
    /// Returns `Err` if the runtime channel is closed.
    pub fn submit_turn(&self, prompt: &str) -> Result<(), String> {
        self.cancel_flag.store(false, Ordering::SeqCst);
        self.cmd_tx
            .send(AgentCommand::SubmitTurn {
                prompt: prompt.to_string(),
            })
            .map_err(|_| "agent runtime channel closed".to_string())
    }

    /// Cancel the current turn. The provider sub-thread will stop
    /// relaying events once it observes the flag.
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        let _ = self.cmd_tx.send(AgentCommand::Cancel);
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
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// The main loop running on the background thread.
fn runtime_loop(
    cmd_rx: Receiver<AgentCommand>,
    event_tx: Sender<AgentEvent>,
    provider: Box<dyn LlmProvider>,
    cancel_flag: Arc<AtomicBool>,
) {
    let mut turn_counter = 0u64;

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            AgentCommand::SubmitTurn { prompt } => {
                turn_counter += 1;
                let turn_id = format!("turn-{turn_counter}");
                info!(
                    "runtime: SubmitTurn id={turn_id} prompt_len={}",
                    prompt.len()
                );

                cancel_flag.store(false, Ordering::SeqCst);

                if event_tx
                    .send(AgentEvent::TurnStarted {
                        turn_id: turn_id.clone(),
                    })
                    .is_err()
                {
                    break;
                }

                run_turn_with_retry(&turn_id, &prompt, &*provider, &event_tx, &cancel_flag);
            }
            AgentCommand::Cancel => {
                debug!("runtime: Cancel received");
                cancel_flag.store(true, Ordering::SeqCst);
            }
            AgentCommand::Shutdown => {
                info!("runtime: Shutdown");
                break;
            }
        }
    }
}

/// Execute a turn with up to `MAX_TRANSIENT_RETRIES` retries on transient errors.
fn run_turn_with_retry(
    turn_id: &str,
    prompt: &str,
    provider: &dyn LlmProvider,
    event_tx: &Sender<AgentEvent>,
    cancel_flag: &Arc<AtomicBool>,
) {
    for attempt in 0..=MAX_TRANSIENT_RETRIES {
        if cancel_flag.load(Ordering::SeqCst) {
            info!("runtime: turn {turn_id} cancelled before attempt {attempt}");
            let _ = event_tx.send(AgentEvent::TurnFailed {
                error: "cancelled".to_string(),
            });
            return;
        }

        let result = run_turn(prompt, provider, event_tx, cancel_flag);

        match result {
            TurnOutcome::Complete | TurnOutcome::Cancelled => return,
            TurnOutcome::TransientError(err) => {
                if attempt < MAX_TRANSIENT_RETRIES {
                    warn!(
                        "runtime: transient error on turn {turn_id} attempt {attempt}: {err}, retrying"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(500));
                } else {
                    warn!("runtime: turn {turn_id} failed after {attempt} retries: {err}");
                    let _ = event_tx.send(AgentEvent::TurnFailed { error: err });
                }
            }
            TurnOutcome::FatalError(err) => {
                warn!("runtime: turn {turn_id} fatal error: {err}");
                let _ = event_tx.send(AgentEvent::TurnFailed { error: err });
                return;
            }
        }
    }
}

enum TurnOutcome {
    Complete,
    Cancelled,
    TransientError(String),
    FatalError(String),
}

/// Classify whether an error is transient (network timeout, 429, 502/503).
fn is_transient_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("timeout")
        || lower.contains("429")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("connection")
        || lower.contains("rate limit")
}

/// Execute a single ReAct turn: stream provider output and relay events.
fn run_turn(
    prompt: &str,
    provider: &dyn LlmProvider,
    event_tx: &Sender<AgentEvent>,
    cancel_flag: &Arc<AtomicBool>,
) -> TurnOutcome {
    let (chunk_tx, chunk_rx) = mpsc::channel::<ProviderEvent>();

    let mut outcome = TurnOutcome::Complete;

    std::thread::scope(|s| {
        s.spawn(|| {
            provider.stream_turn(prompt, chunk_tx);
        });

        let mut got_terminal = false;
        for prov_event in &chunk_rx {
            if cancel_flag.load(Ordering::SeqCst) {
                info!("run_turn: cancel flag observed, stopping relay");
                let _ = event_tx.send(AgentEvent::TurnFailed {
                    error: "cancelled".to_string(),
                });
                outcome = TurnOutcome::Cancelled;
                return;
            }

            let agent_event = match prov_event {
                ProviderEvent::TextDelta(text) => AgentEvent::TextDelta { text },
                ProviderEvent::ThinkingDelta(text) => AgentEvent::ThinkingDelta { text },
                ProviderEvent::ToolCallRequest(payload) => AgentEvent::ToolCallRequest { payload },
                ProviderEvent::TurnComplete => {
                    got_terminal = true;
                    debug!("run_turn: provider sent TurnComplete");
                    AgentEvent::TurnComplete
                }
                ProviderEvent::Error(err) => {
                    warn!("run_turn: provider error: {err}");
                    if is_transient_error(&err) {
                        outcome = TurnOutcome::TransientError(err);
                    } else {
                        outcome = TurnOutcome::FatalError(err);
                    }
                    return;
                }
            };
            if event_tx.send(agent_event).is_err() {
                break;
            }
        }

        if !got_terminal && !matches!(outcome, TurnOutcome::Cancelled) {
            debug!("run_turn: no terminal event from provider, emitting TurnComplete");
            let _ = event_tx.send(AgentEvent::TurnComplete);
        }
    });

    outcome
}
