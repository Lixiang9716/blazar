use crate::agent::protocol::AgentEvent;
use crate::agent::runtime::{AgentRuntime, AgentRuntimeError};
use crate::agent::state::{ActiveToolStatus, AgentRuntimeState, TurnState};
use crate::observability::debug::DebugRecorder;

use crate::chat::input::InputAction;
use crate::chat::model::{Actor, Author, ChatMessage, EntryKind, TimelineEntry, ToolCallStatus};
use crate::chat::picker::ModalPicker;
use crate::chat::theme::ChatTheme;
use crate::chat::users_state::{ContextUsage, StatusMode, UserMode, UsersStatusSnapshot};
use log::{debug, info, trace, warn};
use ratatui_textarea::TextArea;
use std::cell::Cell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

mod actions;
mod events;
pub(crate) mod turns;

#[cfg(test)]
#[path = "../../tests/unit/chat/app/tests.rs"]
mod tests;

pub struct ChatApp {
    messages: Vec<ChatMessage>,
    timeline: Vec<TimelineEntry>,
    composer: TextArea<'static>,
    should_quit: bool,
    display_path: String,
    branch: String,
    scroll_offset: u16,
    show_details: bool,
    pub picker: ModalPicker,
    command_registry: crate::chat::commands::CommandRegistry,
    tick_count: u64,
    /// Remaining demo entries to play back.
    demo_queue: Vec<TimelineEntry>,
    /// When the last demo entry was added (for 1-second pacing).
    demo_last_add: Option<Instant>,
    /// Last known content height of the timeline (set by renderer).
    pub timeline_content_height: Cell<u16>,
    /// Last known visible height of the timeline area (set by renderer).
    pub timeline_visible_height: Cell<u16>,
    theme_name: String,
    theme: ChatTheme,
    agent_runtime: AgentRuntime,
    agent_state: AgentRuntimeState,
    /// Messages queued while agent was busy, dispatched FIFO on turn completion.
    pending_messages: VecDeque<PendingTurn>,
    /// Workspace root for recreating the provider on model switch.
    workspace_root: PathBuf,
    /// Display name of the active model (e.g. "Qwen/Qwen3-8B").
    model_name: String,
    user_mode: UserMode,
    users_status_mode: StatusMode,
    inline_command_matches: Vec<String>,
    git_pr_label: Option<String>,
    referenced_files: Vec<String>,
    context_usage: Option<ContextUsage>,
    /// True once the user has sent at least one message (collapses welcome banner).
    has_user_sent: bool,
    active_turn_kind: Option<TurnKind>,
    active_turn_title: Option<String>,
    thinking_action_name: Option<String>,
    current_turn_has_thinking_entry: bool,
    debug_recorder: DebugRecorder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnKind {
    Chat,
    Plan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingDispatch {
    Runtime {
        runtime_prompt: String,
        kind: TurnKind,
    },
    DiscoverAgents,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingTurn {
    user_text: String,
    dispatch: PendingDispatch,
    timeline_inserted: bool,
}

impl ChatApp {
    pub fn new(repo_path: &str) -> Result<Self, AgentRuntimeError> {
        let display_path = shorten_home(repo_path);
        let branch = detect_branch(repo_path);
        let git_pr_label = infer_pr_label_from_branch(&branch);
        let workspace_root = PathBuf::from(repo_path);

        let timeline = if std::env::var("BLAZAR_DEMO").is_ok() {
            crate::chat::demo::demo_timeline()
        } else {
            vec![
                TimelineEntry {
                    actor: Actor::System,
                    kind: EntryKind::Banner,
                    title: Some("Welcome".to_owned()),
                    body: "Describe a task to get started.".to_owned(),
                    details: String::new(),
                },
                TimelineEntry::response("Tell me what you'd like to explore."),
            ]
        };

        let mut command_registry = crate::chat::commands::CommandRegistry::new();
        crate::chat::commands::builtins::register_builtin_commands(&mut command_registry).map_err(
            |error| {
                AgentRuntimeError::ToolInitialization(format!(
                    "failed to register built-in commands: {error}"
                ))
            },
        )?;

        let (provider, model_name) = crate::provider::load_provider(repo_path);

        Ok(Self {
            messages: vec![ChatMessage {
                author: Author::Spirit,
                body: "Spirit: Tell me what you'd like to explore.".to_owned(),
            }],
            timeline,
            composer: new_composer(),
            display_path,
            branch,
            scroll_offset: u16::MAX, // auto-scroll sentinel
            picker: ModalPicker::command_palette_from_registry(&command_registry),
            command_registry,
            theme_name: crate::chat::theme::DEFAULT_THEME.to_owned(),
            theme: crate::chat::theme::build_theme(),
            agent_runtime: AgentRuntime::new(provider, workspace_root.clone(), model_name.clone())?,
            debug_recorder: DebugRecorder::new(&workspace_root),
            workspace_root,
            model_name,
            git_pr_label,
            user_mode: UserMode::Auto,
            // All remaining fields are zero/empty/false/None defaults.
            should_quit: false,
            show_details: false,
            tick_count: 0,
            demo_queue: Vec::new(),
            demo_last_add: None,
            timeline_content_height: Cell::new(0),
            timeline_visible_height: Cell::new(0),
            agent_state: AgentRuntimeState::default(),
            pending_messages: VecDeque::new(),
            users_status_mode: StatusMode::Normal,
            inline_command_matches: Vec::new(),
            referenced_files: Vec::new(),
            context_usage: None,
            has_user_sent: false,
            active_turn_kind: None,
            active_turn_title: None,
            thinking_action_name: None,
            current_turn_has_thinking_entry: false,
        })
    }

    pub fn new_for_test(_repo_path: &str) -> Result<Self, AgentRuntimeError> {
        let mut app = Self::new(_repo_path)?;
        // Use a fixed display path so snapshots are environment-independent.
        app.display_path = "~/blazar".to_owned();
        // Stable branch for tests.
        app.branch = "main".to_owned();
        app.git_pr_label = infer_pr_label_from_branch(&app.branch);
        // Stable model name for tests.
        app.model_name = "echo".to_owned();
        // Always use EchoProvider in tests — no network calls.
        app.agent_runtime = AgentRuntime::new(
            Box::new(crate::provider::echo::EchoProvider::new(0)),
            std::path::PathBuf::from(_repo_path),
            "echo".to_owned(),
        )?;
        Ok(app)
    }

    /// Creates a ChatApp pre-loaded with demo timeline entries for visual testing.
    #[cfg(test)]
    pub fn new_with_demo_timeline(repo_path: &str) -> Result<Self, AgentRuntimeError> {
        let mut app = Self::new(repo_path)?;
        app.timeline = crate::chat::demo::demo_timeline();
        Ok(app)
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn timeline(&self) -> &[TimelineEntry] {
        &self.timeline
    }

    pub fn display_path(&self) -> &str {
        &self.display_path
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    pub fn status_label(&self) -> String {
        match &self.agent_state.turn_state {
            TurnState::Idle => "ready".to_owned(),
            TurnState::Streaming { .. } => self
                .active_turn_title
                .clone()
                .unwrap_or_else(|| self.derive_active_turn_status_label()),
            TurnState::Done => "ready".to_owned(),
            TurnState::Failed { error } => format!("error: {error}"),
        }
    }

    fn derive_active_turn_status_label(&self) -> String {
        if let Some(tool_name) = self
            .agent_state
            .active_tools
            .iter()
            .rev()
            .find(|tool| tool.status == ActiveToolStatus::Running)
            .map(|tool| tool.tool_name.clone())
        {
            return format!("executing {tool_name}");
        }

        match self.active_turn_kind {
            Some(TurnKind::Plan) => "planning".to_owned(),
            _ => self
                .thinking_action_name
                .clone()
                .unwrap_or_else(|| "thinking".to_owned()),
        }
    }

    pub(crate) fn refresh_active_turn_status_label(&mut self) {
        self.active_turn_title = match self.agent_state.turn_state {
            TurnState::Streaming { .. } => Some(self.derive_active_turn_status_label()),
            _ => None,
        };
    }

    pub fn is_streaming(&self) -> bool {
        matches!(self.agent_state.turn_state, TurnState::Streaming { .. })
    }

    pub fn is_failed(&self) -> bool {
        matches!(self.agent_state.turn_state, TurnState::Failed { .. })
    }

    /// Cancel the current streaming turn.
    pub fn cancel_turn(&mut self) {
        if self.is_streaming() {
            info!("cancel_turn: cancelling current turn");
            self.agent_runtime.cancel();
        }
    }

    pub fn show_details(&self) -> bool {
        self.show_details
    }

    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    pub fn theme(&self) -> &ChatTheme {
        &self.theme
    }

    pub fn theme_name(&self) -> &str {
        &self.theme_name
    }

    pub fn set_theme(&mut self, name: &str) {
        self.theme = crate::chat::theme::build_theme_by_name(name);
        self.theme_name = name.to_owned();
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn users_status_snapshot(&self) -> UsersStatusSnapshot {
        UsersStatusSnapshot {
            mode: self.user_mode,
            status_mode: self.users_status_mode,
            current_path: self.display_path.clone(),
            branch: self.branch.clone(),
            pr_label: self.git_pr_label.clone(),
            referenced_files: self.referenced_files.clone(),
            model_name: self.model_name.clone(),
            context_usage: self.context_usage,
        }
    }

    pub fn debug_status_label(&self) -> String {
        self.debug_recorder
            .status_summary(self.pending_messages.len())
    }

    pub(crate) fn inline_command_matches(&self) -> &[String] {
        &self.inline_command_matches
    }

    pub(crate) fn ingest_referenced_files_from_claims(&mut self, normalized_claims: &[String]) {
        const MAX_REFERENCED_FILES: usize = 8;

        for claim in normalized_claims {
            let Some(path) = parse_workspace_claim_path(claim) else {
                continue;
            };
            if self
                .referenced_files
                .iter()
                .any(|existing| existing == path)
            {
                continue;
            }
            self.referenced_files.push(path.to_owned());
        }

        if self.referenced_files.len() > MAX_REFERENCED_FILES {
            let overflow = self.referenced_files.len() - MAX_REFERENCED_FILES;
            self.referenced_files.drain(..overflow);
        }
    }

    /// Whether the user has sent at least one message this session.
    pub fn has_user_sent(&self) -> bool {
        self.has_user_sent
    }

    pub(crate) fn queued_user_texts_for_render(&self) -> Vec<String> {
        self.pending_messages
            .iter()
            .map(|turn| turn.user_text.clone())
            .collect()
    }

    /// Switch the active LLM model by rebuilding the provider and agent runtime.
    ///
    /// Cancels any in-flight turn, reloads config from disk with the new model
    /// name, and creates a fresh `AgentRuntime`. Conversation history is reset.
    pub fn set_model(&mut self, model: &str) {
        if self.is_streaming() {
            self.agent_runtime.cancel();
        }

        match self.agent_runtime.set_model(model) {
            Ok(()) => {
                self.model_name = model.to_owned();
                self.timeline.push(TimelineEntry::hint(format!(
                    "Model switched to **{model}**"
                )));
                self.scroll_offset = u16::MAX;
            }
            Err(e) => {
                self.timeline.push(TimelineEntry::warning(format!(
                    "Failed to switch model: {e}"
                )));
                self.scroll_offset = u16::MAX;
            }
        }
    }

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        self.picker
            .overlay_state_mut()
            .tick(std::time::Duration::from_millis(100));

        // Drain agent runtime events
        while let Some(event) = self.agent_runtime.try_recv() {
            self.apply_agent_event(event);
        }

        // Demo playback: add one entry per second
        if !self.demo_queue.is_empty() {
            let should_add = match self.demo_last_add {
                Some(last) => last.elapsed().as_secs() >= 1,
                None => true,
            };
            if should_add {
                let entry = self.demo_queue.remove(0);
                self.timeline.push(entry);
                self.scroll_offset = u16::MAX; // auto-scroll
                self.demo_last_add = Some(Instant::now());
            }
        }
    }

    /// Whether demo playback is currently running.
    pub fn demo_active(&self) -> bool {
        !self.demo_queue.is_empty()
    }

    /// Convert the u16::MAX auto-scroll sentinel into a real offset
    /// so that manual scroll adjustments work correctly.
    fn resolve_scroll_sentinel(&mut self) {
        if self.scroll_offset == u16::MAX {
            self.scroll_offset = self
                .timeline_content_height
                .get()
                .saturating_sub(self.timeline_visible_height.get());
        }
    }

    pub fn set_composer_text(&mut self, value: &str) {
        self.composer = TextArea::from([value.to_owned()]);
        self.sync_users_status_from_composer();
    }

    pub fn composer_text(&self) -> String {
        self.composer.lines().join("\n")
    }

    pub fn submit_composer(&mut self) {
        let text = self.composer_text();
        self.send_message(&text);
        self.composer = new_composer();
        self.sync_users_status_from_composer();
    }

    pub(crate) fn sync_users_status_from_composer(&mut self) {
        let query = self.composer_text();
        if query.starts_with('/') {
            self.users_status_mode = StatusMode::CommandList;
            self.refresh_inline_command_matches(&query);
        } else {
            self.users_status_mode = StatusMode::Normal;
            self.inline_command_matches.clear();
        }
    }

    pub(crate) fn normalized_slash_query(&self) -> String {
        normalize_slash_query(&self.composer_text())
    }

    fn refresh_inline_command_matches(&mut self, query: &str) {
        let normalized_query = normalize_slash_query(query);
        let command_specs: Vec<crate::chat::commands::CommandSpec> =
            self.command_registry.list().into_iter().cloned().collect();
        self.inline_command_matches =
            crate::chat::commands::matcher::ranked_match_names(&normalized_query, &command_specs)
                .into_iter()
                .take(6)
                .map(str::to_owned)
                .collect();
    }

    fn active_turn_kind_label(&self) -> Option<&'static str> {
        match self.active_turn_kind {
            Some(TurnKind::Chat) => Some("chat"),
            Some(TurnKind::Plan) => Some("plan"),
            None => None,
        }
    }

    pub(crate) fn execute_palette_command_sync(
        &mut self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<crate::chat::commands::CommandResult, crate::chat::commands::CommandError> {
        let command = self.command_registry.find(name).cloned().ok_or_else(|| {
            crate::chat::commands::CommandError::Unavailable(format!("unknown command: {name}"))
        })?;
        let exec_future = crate::chat::commands::orchestrator::execute_palette_command_from_command(
            command, self, args,
        );
        if tokio::runtime::Handle::try_current().is_ok() {
            return futures::executor::block_on(exec_future);
        }

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                crate::chat::commands::CommandError::ExecutionFailed(format!(
                    "failed to initialize tokio runtime: {error}"
                ))
            })?;
        runtime.block_on(exec_future)
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn composer_mut(&mut self) -> &mut TextArea<'static> {
        &mut self.composer
    }

    pub fn composer(&self) -> &TextArea<'static> {
        &self.composer
    }
}

/// Shorten `/home/<user>/...` to `~/...` for display.
fn shorten_home(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME")
        && let Some(rest) = path.strip_prefix(&home)
    {
        return format!("~{rest}");
    }
    path.to_owned()
}

fn new_composer() -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(ratatui_core::style::Style::default());
    ta
}

pub(crate) fn normalize_slash_query(query: &str) -> String {
    query
        .replace(['\r', '\n'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn infer_pr_label_from_branch(branch: &str) -> Option<String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return None;
    }

    let segments = branch
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    for (index, segment) in segments.iter().enumerate() {
        if let Some(number) = segment.strip_prefix("pr-").and_then(leading_digits) {
            return Some(format!("PR#{number}"));
        }
        if let Some(number) = segment.strip_prefix("pr_").and_then(leading_digits) {
            return Some(format!("PR#{number}"));
        }
        if matches!(*segment, "pr" | "pull")
            && let Some(number) = segments
                .get(index + 1)
                .and_then(|next| leading_digits(next))
        {
            return Some(format!("PR#{number}"));
        }
    }

    let hash_suffix = branch.rsplit_once('#').map(|(_, suffix)| suffix)?;
    let number = leading_digits(hash_suffix)?;
    Some(format!("PR#{number}"))
}

fn leading_digits(value: &str) -> Option<&str> {
    let end = value
        .char_indices()
        .find_map(|(index, ch)| (!ch.is_ascii_digit()).then_some(index))
        .unwrap_or(value.len());
    (end > 0).then_some(&value[..end])
}

fn parse_workspace_claim_path(claim: &str) -> Option<&str> {
    let (resource, _) = claim.split_once('#')?;
    let path = resource.strip_prefix("fs:")?;
    (!path.is_empty()).then_some(path)
}

fn preview_text(text: &str, max_chars: usize) -> &str {
    if text.chars().count() <= max_chars {
        return text;
    }

    let end = text
        .char_indices()
        .nth(max_chars)
        .map(|(index, _)| index)
        .unwrap_or(text.len());

    &text[..end]
}

fn summarize_tool_arguments(arguments: &str) -> String {
    preview_text(arguments, 60).to_owned()
}

fn summarize_tool_output(output: &str) -> String {
    let first_line = output.lines().next().unwrap_or("");
    preview_text(first_line, 80).to_owned()
}

/// Detect the current git branch. Returns empty string if not in a git repo.
fn detect_branch(repo_path: &str) -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_owned())
            } else {
                None
            }
        })
        .unwrap_or_default()
}

#[cfg(test)]
use turns::build_pending_turn;
