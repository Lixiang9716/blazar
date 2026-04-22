use std::fmt::{self, Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use log::{debug, info, warn};
use std::collections::HashSet;

use super::acp_discovery::{AcpDiscovery, AcpTransport, ReqwestAcpTransport, normalize_endpoint};
use super::protocol::{AgentCommand, AgentEvent};
use super::tools::ToolRegistry;
use crate::agent::tools::acp::AcpAgentTool;
use crate::agent::tools::agent::AgentTool;
use crate::agent::tools::bash::BashTool;
use crate::agent::tools::list_dir::ListDirTool;
use crate::agent::tools::read_file::ReadFileTool;
use crate::agent::tools::write_file::WriteFileTool;
use crate::config::{AGENTS_CONFIG_PATH, load_agents_config_from_path};
use crate::provider::{LlmProvider, ProviderMessage};

mod json_repair;
pub(crate) mod turn;

#[cfg(test)]
mod tests;

use turn::{ChannelObserver, TurnOutcome, execute_turn};

#[cfg(test)]
use crate::provider::ProviderEvent;
#[cfg(test)]
use json_repair::{
    extract_json_payload, parse_or_repair_json, preview_text, repair_control_chars,
    strip_thinking_tags,
};
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use turn::{is_transient_error, stream_provider_pass};

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
const MAX_TOOL_ITERATIONS: usize = 10;
const REPEATED_SUCCESS_GUIDANCE: &str = "REPEATED SUCCESS: identical tool call already succeeded in this turn. \
     Stop repeating it and continue with the next step or final answer.";
const JSON_REPAIR_NOTE: &str = "[NOTE] Tool arguments had malformed JSON and were auto-repaired. \
Use escaped double quotes (\\\") inside JSON string values.";
const TIMEOUT_NOTE: &str = "TIMEOUT NOTE: this command exceeded the tool timeout. \
If this is computation-heavy, the algorithm may be too slow for the current input. \
Consider a more efficient approach (e.g., memoization/iterative rewrite), reducing input size, or only then increasing timeout_secs.";
const REPEATED_TIMEOUT_GUIDANCE: &str = "REPEATED TIMEOUT: the same tool call timed out multiple times. \
Change strategy instead of retrying the same implementation.";

type RuntimeWorker = Box<dyn FnOnce() + Send + 'static>;

#[derive(Debug)]
pub enum AgentRuntimeError {
    ThreadSpawn(io::Error),
    ToolInitialization(String),
}

impl Display for AgentRuntimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ThreadSpawn(source) => {
                write!(f, "failed to spawn agent runtime thread: {source}")
            }
            Self::ToolInitialization(message) => {
                write!(f, "failed to initialize agent tools: {message}")
            }
        }
    }
}

impl std::error::Error for AgentRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ThreadSpawn(source) => Some(source),
            Self::ToolInitialization(_) => None,
        }
    }
}

impl AgentRuntime {
    /// Create a new runtime with the given provider and default model.
    pub fn new(
        provider: Box<dyn LlmProvider>,
        workspace_root: PathBuf,
        model: String,
    ) -> Result<Self, AgentRuntimeError> {
        Self::new_with_spawner(provider, workspace_root, model, spawn_worker_thread)
    }

    fn new_with_spawner<F>(
        provider: Box<dyn LlmProvider>,
        workspace_root: PathBuf,
        model: String,
        spawn_thread: F,
    ) -> Result<Self, AgentRuntimeError>
    where
        F: FnOnce(RuntimeWorker) -> io::Result<JoinHandle<()>>,
    {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&cancel_flag);
        let provider: Arc<dyn LlmProvider> = Arc::from(provider);
        let tools = build_tool_registry(Arc::clone(&provider), &workspace_root, &model)
            .map_err(AgentRuntimeError::ToolInitialization)?;
        let worker: RuntimeWorker =
            Box::new(move || runtime_loop(cmd_rx, event_tx, provider, model, tools, flag_clone));

        let handle = spawn_thread(worker).map_err(AgentRuntimeError::ThreadSpawn)?;

        Ok(Self {
            cmd_tx,
            event_rx,
            cancel_flag,
            handle: Some(handle),
        })
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

    /// Switch the active model without rebuilding the runtime.
    /// Conversation history is preserved.
    pub fn set_model(&self, model: &str) -> Result<(), String> {
        self.cmd_tx
            .send(AgentCommand::SetModel {
                model: model.to_string(),
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

fn spawn_worker_thread(worker: RuntimeWorker) -> io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("blazar-agent".into())
        .spawn(worker)
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
    provider: Arc<dyn LlmProvider>,
    mut model: String,
    tools: ToolRegistry,
    cancel_flag: Arc<AtomicBool>,
) {
    let mut turn_counter = 0u64;
    let mut conversation_history: Vec<ProviderMessage> = Vec::new();

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            AgentCommand::SubmitTurn { prompt } => {
                turn_counter += 1;
                let turn_id = format!("turn-{turn_counter}");
                info!(
                    "runtime: SubmitTurn id={turn_id} model={model} prompt_len={}",
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

                if let Some(updated_history) = run_turn_with_retry(
                    &turn_id,
                    &prompt,
                    &conversation_history,
                    &*provider,
                    &model,
                    &tools,
                    &event_tx,
                    &cancel_flag,
                ) {
                    conversation_history = updated_history;
                }
            }
            AgentCommand::SetModel { model: new_model } => {
                info!("runtime: SetModel old={model} new={new_model}");
                model = new_model;
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
#[allow(clippy::too_many_arguments)]
fn run_turn_with_retry(
    turn_id: &str,
    prompt: &str,
    history: &[ProviderMessage],
    provider: &dyn LlmProvider,
    model: &str,
    tools: &ToolRegistry,
    event_tx: &Sender<AgentEvent>,
    cancel_flag: &Arc<AtomicBool>,
) -> Option<Vec<ProviderMessage>> {
    for attempt in 0..=MAX_TRANSIENT_RETRIES {
        if cancel_flag.load(Ordering::SeqCst) {
            info!("runtime: turn {turn_id} cancelled before attempt {attempt}");
            let _ = event_tx.send(AgentEvent::TurnFailed {
                error: "cancelled".to_string(),
            });
            return None;
        }

        let mut messages = history.to_vec();
        messages.push(ProviderMessage::User {
            content: prompt.to_string(),
        });
        let observer = ChannelObserver { tx: event_tx };
        let result = execute_turn(
            &mut messages,
            provider,
            model,
            tools,
            &observer,
            cancel_flag,
        );

        match result {
            TurnOutcome::Complete => {
                let _ = event_tx.send(AgentEvent::TurnComplete);
                return Some(messages);
            }
            TurnOutcome::Cancelled => return None,
            TurnOutcome::TransientError(err) => {
                if attempt < MAX_TRANSIENT_RETRIES {
                    warn!(
                        "runtime: transient error on turn {turn_id} attempt {attempt}: {err}, retrying"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(500));
                } else {
                    warn!("runtime: turn {turn_id} failed after {attempt} retries: {err}");
                    let _ = event_tx.send(AgentEvent::TurnFailed { error: err });
                    return None;
                }
            }
            TurnOutcome::FatalError(err) => {
                warn!("runtime: turn {turn_id} fatal error: {err}");
                let _ = event_tx.send(AgentEvent::TurnFailed { error: err });
                return None;
            }
        }
    }

    None
}

fn build_tool_registry(
    provider: Arc<dyn LlmProvider>,
    workspace_root: &Path,
    model: &str,
) -> Result<ToolRegistry, String> {
    let mut tools = ToolRegistry::new(workspace_root.to_path_buf());
    tools.register(Box::new(ReadFileTool::new(workspace_root.to_path_buf())));
    tools.register(Box::new(WriteFileTool::new(workspace_root.to_path_buf())));
    tools.register(Box::new(ListDirTool::new(workspace_root.to_path_buf())));
    tools.register(Box::new(BashTool::new(workspace_root.to_path_buf())));
    tools.register(Box::new(AgentTool::new(
        "sub_agent",
        "Delegate a task to a sub-agent that can read files, write files, list directories, and run bash commands. Use when the task is self-contained and benefits from independent reasoning.",
        Arc::clone(&provider),
        model,
        workspace_root.to_path_buf(),
    )));

    register_acp_tools(&mut tools, workspace_root)?;

    Ok(tools)
}

fn register_acp_tools(tools: &mut ToolRegistry, workspace_root: &Path) -> Result<(), String> {
    let config_path = workspace_root.join(AGENTS_CONFIG_PATH);
    if !config_path.exists() {
        return Ok(());
    }

    let config = load_agents_config_from_path(&config_path)
        .map_err(|error| format!("{}: {error}", config_path.display()))?;
    let transport = ReqwestAcpTransport::new().map_err(|error| error.to_string())?;
    let mut seen_agent_keys = HashSet::new();
    let mut seen_tool_names = HashSet::new();

    for configured in config.agents.into_iter().filter(|agent| agent.enabled) {
        let metadata = transport
            .get_agent(&configured.endpoint, &configured.agent_id)
            .map_err(|error| error.to_string())?;
        if !seen_tool_names.insert(configured.name.clone()) {
            return Err(format!("duplicate ACP tool name: {}", configured.name));
        }
        seen_agent_keys.insert((
            normalize_endpoint(&configured.endpoint),
            configured.agent_id,
        ));
        tools.register(Box::new(AcpAgentTool::with_transport(
            configured.name,
            normalize_endpoint(&configured.endpoint),
            metadata,
            transport.clone(),
        )));
    }

    let discovery = AcpDiscovery::new(config.discovery.endpoints, transport.clone());
    let report = discovery.discover(&seen_agent_keys);
    if !report.errors.is_empty() {
        let message = report
            .errors
            .into_iter()
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("ACP discovery failed: {message}"));
    }
    for discovered in report.agents {
        let tool_name = discovered.metadata.name.clone();
        if !seen_tool_names.insert(tool_name.clone()) {
            warn!("runtime: skipped discovered ACP agent with duplicate tool name {tool_name}");
            continue;
        }
        tools.register(Box::new(AcpAgentTool::with_transport(
            tool_name,
            discovered.endpoint,
            discovered.metadata,
            transport.clone(),
        )));
    }

    Ok(())
}
