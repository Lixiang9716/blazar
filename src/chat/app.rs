use crate::agent::protocol::AgentEvent;
use crate::agent::runtime::AgentRuntime;
use crate::agent::state::{AgentRuntimeState, TurnState};

use crate::chat::input::InputAction;
use crate::chat::model::{Actor, Author, ChatMessage, EntryKind, TimelineEntry, ToolCallStatus};
use crate::chat::picker::{ModalPicker, PickerContext, PickerItem};
use crate::chat::theme::ChatTheme;
use crate::provider::LlmProvider;
use crate::provider::echo::EchoProvider;
use crate::provider::siliconflow::SiliconFlowConfig;
use log::{debug, info, trace, warn};
use ratatui_textarea::TextArea;
use std::cell::Cell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

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
    /// True once the user has sent at least one message (collapses welcome banner).
    has_user_sent: bool,
    active_turn_kind: Option<TurnKind>,
    active_turn_title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnKind {
    Chat,
    Plan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingTurn {
    user_text: String,
    runtime_prompt: String,
    kind: TurnKind,
}

impl ChatApp {
    pub fn new(repo_path: &str) -> Self {
        let display_path = shorten_home(repo_path);
        let branch = detect_branch(repo_path);

        let timeline = if std::env::var("BLAZAR_DEMO").is_ok() {
            crate::chat::demo::demo_timeline()
        } else {
            vec![TimelineEntry::response(
                "Tell me what you'd like to explore.",
            )]
        };

        let theme = crate::chat::theme::build_theme();

        // Try SiliconFlow provider; fall back to EchoProvider.
        let (provider, model_name): (Box<dyn LlmProvider>, String) =
            match SiliconFlowConfig::load(repo_path) {
                Ok(cfg) => {
                    let name = cfg.model.clone();
                    (
                        Box::new(crate::provider::siliconflow::SiliconFlowProvider::new(cfg)),
                        name,
                    )
                }
                Err(_) => (Box::new(EchoProvider::default()), "echo".to_owned()),
            };

        let workspace_root = PathBuf::from(repo_path);

        Self {
            messages: vec![ChatMessage {
                author: Author::Spirit,
                body: "Spirit: Tell me what you'd like to explore.".to_owned(),
            }],
            timeline,
            composer: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(ratatui_core::style::Style::default());
                ta
            },
            should_quit: false,
            display_path,
            branch,
            scroll_offset: u16::MAX, // auto-scroll sentinel
            show_details: false,
            picker: ModalPicker::command_palette(),
            tick_count: 0,
            demo_queue: Vec::new(),
            demo_last_add: None,
            timeline_content_height: Cell::new(0),
            timeline_visible_height: Cell::new(0),
            theme_name: crate::chat::theme::DEFAULT_THEME.to_owned(),
            theme,
            agent_runtime: AgentRuntime::new(provider, workspace_root.clone()),
            agent_state: AgentRuntimeState::default(),
            pending_messages: VecDeque::new(),
            workspace_root,
            model_name,
            has_user_sent: false,
            active_turn_kind: None,
            active_turn_title: None,
        }
    }

    pub fn new_for_test(_repo_path: &str) -> Self {
        let mut app = Self::new(_repo_path);
        // Use a fixed display path so snapshots are environment-independent.
        app.display_path = "~/blazar".to_owned();
        // Stable branch for tests.
        app.branch = "main".to_owned();
        // Stable model name for tests.
        app.model_name = "echo".to_owned();
        // Always use EchoProvider in tests — no network calls.
        app.agent_runtime = AgentRuntime::new(
            Box::new(EchoProvider::new(0)),
            std::path::PathBuf::from(_repo_path),
        );
        app
    }

    /// Creates a ChatApp pre-loaded with demo timeline entries for visual testing.
    #[cfg(test)]
    pub fn new_with_demo_timeline(repo_path: &str) -> Self {
        let mut app = Self::new(repo_path);
        app.timeline = crate::chat::demo::demo_timeline();
        app
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
            TurnState::Streaming { .. } => {
                self.active_turn_title
                    .clone()
                    .unwrap_or_else(|| match self.active_turn_kind {
                        Some(TurnKind::Plan) => "thinking".to_owned(),
                        _ => "streaming…".to_owned(),
                    })
            }
            TurnState::Done => "ready".to_owned(),
            TurnState::Failed { error } => format!("error: {error}"),
        }
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

    /// Whether the user has sent at least one message this session.
    pub fn has_user_sent(&self) -> bool {
        self.has_user_sent
    }

    /// Switch the active LLM model by rebuilding the provider and agent runtime.
    ///
    /// Cancels any in-flight turn, reloads config from disk with the new model
    /// name, and creates a fresh `AgentRuntime`. Conversation history is reset.
    pub fn set_model(&mut self, model: &str) {
        if self.is_streaming() {
            self.agent_runtime.cancel();
        }

        let repo_str = self.workspace_root.to_string_lossy();
        match SiliconFlowConfig::load(&repo_str) {
            Ok(mut cfg) => {
                cfg.model = model.to_owned();
                let provider: Box<dyn LlmProvider> =
                    Box::new(crate::provider::siliconflow::SiliconFlowProvider::new(cfg));
                self.agent_runtime = AgentRuntime::new(provider, self.workspace_root.clone());
                self.agent_state = AgentRuntimeState::default();
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

    pub fn send_message(&mut self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }

        let turn = build_pending_turn(trimmed);

        info!(
            "send_message: len={} preview={:.60}",
            trimmed.len(),
            trimmed
        );

        // Trigger demo playback when user types "1"
        if trimmed == "1" {
            self.timeline.clear();
            self.demo_queue = crate::chat::demo::demo_playback_script();
            self.demo_last_add = None;
            self.scroll_offset = u16::MAX;
            return;
        }

        self.messages.push(ChatMessage {
            author: Author::User,
            body: turn.user_text.clone(),
        });

        self.has_user_sent = true;

        // Add user message to timeline
        self.timeline
            .push(TimelineEntry::user_message(turn.user_text.clone()));

        // Admission control: queue if agent is busy instead of dropping
        if self.agent_state.is_busy() {
            info!(
                "send_message: queued (agent busy) queue_depth={}",
                self.pending_messages.len() + 1
            );
            self.pending_messages.push_back(turn);
            return;
        }

        // Dispatch to agent runtime — response arrives via events in tick()
        self.active_turn_kind = Some(turn.kind);
        self.active_turn_title = self.streaming_title_for_turn(turn.kind);
        if let Err(e) = self.agent_runtime.submit_turn(&turn.runtime_prompt) {
            warn!("send_message: submit_turn failed: {e}");
            self.active_turn_kind = None;
            self.active_turn_title = None;
            self.timeline
                .push(TimelineEntry::warning(format!("Runtime error: {e}")));
        }

        // Auto-scroll to bottom
        self.scroll_offset = u16::MAX;
    }

    /// Dispatches the next queued message to the agent runtime (FIFO).
    /// Called after any terminal turn event (TurnComplete, TurnFailed).
    fn dispatch_next_queued(&mut self) {
        if let Some(turn) = self.pending_messages.pop_front() {
            info!(
                "dispatch_next_queued: dispatching len={} remaining={}",
                turn.user_text.len(),
                self.pending_messages.len()
            );
            self.active_turn_kind = Some(turn.kind);
            self.active_turn_title = self.streaming_title_for_turn(turn.kind);
            if let Err(e) = self.agent_runtime.submit_turn(&turn.runtime_prompt) {
                warn!("dispatch_next_queued: submit_turn failed: {e}");
                self.active_turn_kind = None;
                self.active_turn_title = None;
                self.timeline
                    .push(TimelineEntry::warning(format!("Runtime error: {e}")));
                // Don't re-queue on channel error — runtime is dead
            }
            self.scroll_offset = u16::MAX;
        }
    }

    pub fn set_composer_text(&mut self, value: &str) {
        self.composer = TextArea::from([value.to_owned()]);
    }

    pub fn composer_text(&self) -> String {
        self.composer.lines().join("\n")
    }

    pub fn submit_composer(&mut self) {
        let text = self.composer_text();
        self.send_message(&text);
        self.composer = TextArea::default();
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn handle_action(&mut self, action: InputAction) {
        // When picker is open, route input to it
        if self.picker.is_open() {
            match action {
                InputAction::Quit => {
                    self.picker.close();
                }
                InputAction::Submit => {
                    if let Some(cmd) = self.picker.select_current() {
                        let ctx = self.picker.context;
                        self.picker.close();

                        // Sub-picker selection (no / prefix) — dispatch by context
                        if !cmd.starts_with('/') {
                            match ctx {
                                PickerContext::ThemeSelect => {
                                    self.set_theme(&cmd);
                                }
                                PickerContext::ModelSelect => {
                                    let clean = cmd.trim_end_matches(" ✓");
                                    self.set_model(clean);
                                }
                                PickerContext::Commands => {
                                    self.send_message(&cmd);
                                }
                            }
                            self.picker = ModalPicker::command_palette();
                            return;
                        }

                        // /theme selected — open theme sub-picker
                        if cmd == "/theme" {
                            let theme_items: Vec<PickerItem> =
                                crate::chat::theme::available_themes()
                                    .into_iter()
                                    .map(|info| {
                                        PickerItem::new(
                                            info.name.clone(),
                                            info.display_name.clone(),
                                        )
                                    })
                                    .collect();
                            self.picker = ModalPicker::with_context(
                                "Select Theme",
                                theme_items,
                                PickerContext::ThemeSelect,
                            );
                            self.picker.open();
                            return;
                        }

                        // /model selected — open model sub-picker
                        if cmd == "/model" {
                            use crate::provider::siliconflow::POPULAR_MODELS;
                            let current = &self.model_name;
                            let model_items: Vec<PickerItem> = POPULAR_MODELS
                                .iter()
                                .map(|(name, desc)| {
                                    let label = if *name == current {
                                        format!("{name} ✓")
                                    } else {
                                        name.to_string()
                                    };
                                    PickerItem::new(label, *desc)
                                })
                                .collect();
                            self.picker = ModalPicker::with_context(
                                "Select Model",
                                model_items,
                                PickerContext::ModelSelect,
                            );
                            self.picker.open();
                            return;
                        }

                        if cmd == "/plan" {
                            self.set_composer_text("/plan ");
                            return;
                        }

                        self.send_message(&cmd);
                    }
                }
                InputAction::ScrollUp => self.picker.move_up(),
                InputAction::ScrollDown => self.picker.move_down(),
                InputAction::PickerUp => self.picker.move_up(),
                InputAction::PickerDown => self.picker.move_down(),
                InputAction::Backspace => {
                    if self.picker.filter.is_empty() {
                        self.picker.close();
                    } else {
                        self.picker.pop_filter();
                    }
                }
                InputAction::Key(key) => {
                    if let crossterm::event::KeyCode::Char(ch) = key.code {
                        self.picker.push_filter(ch);
                    }
                }
                _ => {}
            }
            return;
        }

        match action {
            InputAction::Quit => {
                if self.is_streaming() {
                    self.cancel_turn();
                } else {
                    self.should_quit = true;
                }
            }
            InputAction::Submit => self.submit_composer(),
            InputAction::ToggleDetails => self.show_details = !self.show_details,
            InputAction::ScrollUp => {
                self.resolve_scroll_sentinel();
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
            }
            InputAction::ScrollDown => {
                self.resolve_scroll_sentinel();
                self.scroll_offset = self.scroll_offset.saturating_add(3);
            }
            InputAction::Key(key) => {
                // Open command palette when typing '/' in empty composer
                if let crossterm::event::KeyCode::Char('/') = key.code
                    && self.composer_text().is_empty()
                {
                    self.picker.open();
                    return;
                }
                self.composer.input(key);
            }
            InputAction::Backspace => {
                self.composer.input(crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Backspace,
                    crossterm::event::KeyModifiers::NONE,
                ));
            }
            InputAction::Paste(text) => {
                debug!("handle_action: paste len={}", text.len());
                self.composer.insert_str(&text);
            }
            _ => {}
        }
    }

    pub fn composer_mut(&mut self) -> &mut TextArea<'static> {
        &mut self.composer
    }

    pub fn composer(&self) -> &TextArea<'static> {
        &self.composer
    }

    #[doc(hidden)]
    pub fn apply_agent_event_for_test(&mut self, event: AgentEvent) {
        self.apply_agent_event(event);
    }

    fn apply_agent_event(&mut self, event: AgentEvent) {
        let _ = self.agent_state.apply_event(&event);

        match event {
            AgentEvent::TurnStarted { .. } => {
                debug!("tick: TurnStarted");
                self.scroll_offset = u16::MAX;
            }
            AgentEvent::ThinkingDelta { text } => {
                trace!("tick: ThinkingDelta len={}", text.len());
                let needs_new = self
                    .timeline
                    .last()
                    .is_none_or(|entry| entry.kind != EntryKind::Thinking);
                if needs_new {
                    self.timeline.push(TimelineEntry::thinking(""));
                }
                if let Some(last) = self.timeline.last_mut() {
                    last.body.push_str(&text);
                    last.details.push_str(&text);
                }
                self.scroll_offset = u16::MAX;
            }
            AgentEvent::TextDelta { text } => {
                trace!("tick: TextDelta len={}", text.len());
                let needs_new = self.timeline.last().is_none_or(|entry| {
                    !(entry.actor == Actor::Assistant && entry.kind == EntryKind::Message)
                });
                if needs_new {
                    self.timeline.push(TimelineEntry::response(""));
                }
                if let Some(last) = self.timeline.last_mut() {
                    last.body.push_str(&text);
                }
                self.scroll_offset = u16::MAX;
            }
            AgentEvent::ToolCallStarted {
                call_id,
                tool_name,
                arguments,
            } => {
                debug!(
                    "tick: ToolCallStarted call_id={} tool={} arguments_len={}",
                    call_id,
                    tool_name,
                    arguments.len()
                );
                self.timeline.push(TimelineEntry::tool_call(
                    call_id,
                    tool_name,
                    summarize_tool_arguments(&arguments),
                    arguments,
                    ToolCallStatus::Running,
                ));
                self.scroll_offset = u16::MAX;
            }
            AgentEvent::ToolCallCompleted {
                call_id,
                output,
                is_error,
            } => {
                debug!(
                    "tick: ToolCallCompleted call_id={} is_error={} output_len={}",
                    call_id,
                    is_error,
                    output.len()
                );
                if let Some(entry) = self.timeline.iter_mut().rev().find(|entry| {
                    matches!(
                        &entry.kind,
                        EntryKind::ToolCall { call_id: existing, .. } if existing == &call_id
                    )
                }) {
                    entry.body = summarize_tool_output(&output);
                    entry.details = output;
                    if let EntryKind::ToolCall { status, .. } = &mut entry.kind {
                        *status = if is_error {
                            ToolCallStatus::Error
                        } else {
                            ToolCallStatus::Success
                        };
                    }
                }
                self.scroll_offset = u16::MAX;
            }
            AgentEvent::TurnComplete => {
                debug!("tick: TurnComplete");
                if self.active_turn_kind == Some(TurnKind::Plan) {
                    self.finalize_plan_response();
                }
                self.active_turn_kind = None;
                self.active_turn_title = None;
                self.dispatch_next_queued();
            }
            AgentEvent::TurnFailed { error } => {
                if error == "cancelled" {
                    debug!("tick: TurnCancelled");
                    self.timeline.push(TimelineEntry::hint("Turn cancelled."));
                } else {
                    warn!("tick: TurnFailed error={error}");
                    self.timeline
                        .push(TimelineEntry::warning(format!("Agent error: {error}")));
                }
                self.active_turn_kind = None;
                self.active_turn_title = None;
                self.dispatch_next_queued();
                self.scroll_offset = u16::MAX;
            }
        }
    }

    fn finalize_plan_response(&mut self) {
        let Some(entry) = self
            .timeline
            .iter_mut()
            .rev()
            .find(|entry| entry.actor == Actor::Assistant && entry.kind == EntryKind::Message)
        else {
            return;
        };

        let Some((title, body)) = extract_plan_title_and_body(&entry.body) else {
            return;
        };

        entry.title = Some(title);
        entry.body = body;
    }

    fn streaming_title_for_turn(&self, kind: TurnKind) -> Option<String> {
        match kind {
            TurnKind::Plan => None,
            TurnKind::Chat => self.latest_plan_title(),
        }
    }

    fn latest_plan_title(&self) -> Option<String> {
        self.timeline
            .iter()
            .rev()
            .find_map(|entry| (entry.actor == Actor::Assistant).then(|| entry.title.clone()))
            .flatten()
    }
}

fn build_pending_turn(input: &str) -> PendingTurn {
    let trimmed = input.trim();
    if let Some(request) = trimmed.strip_prefix("/plan") {
        return PendingTurn {
            user_text: trimmed.to_owned(),
            runtime_prompt: build_plan_prompt(request.trim()),
            kind: TurnKind::Plan,
        };
    }

    PendingTurn {
        user_text: trimmed.to_owned(),
        runtime_prompt: trimmed.to_owned(),
        kind: TurnKind::Chat,
    }
}

fn build_plan_prompt(request: &str) -> String {
    format!(
        "You are in planning mode.\n\
         Generate a concise implementation plan only.\n\
         First line must be a short plain-text title with no markdown, no numbering, and no label.\n\
         After a blank line, write the plan body as concise ordered steps.\n\
         Keep the answer focused on planning.\n\n\
         User request:\n{request}"
    )
}

fn extract_plan_title_and_body(text: &str) -> Option<(String, String)> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut lines = trimmed.lines();
    let title = lines.next()?.trim().trim_matches('#').trim().to_owned();
    if title.is_empty() {
        return None;
    }

    let body = lines.collect::<Vec<_>>().join("\n").trim().to_owned();
    Some((title, body))
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
mod tests {
    use super::*;
    use crate::chat::view::render_to_lines_for_test;
    use crate::provider::{ProviderEvent, ProviderMessage};
    use std::path::PathBuf;
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct UnicodeArgumentProvider;

    impl LlmProvider for UnicodeArgumentProvider {
        fn stream_turn(
            &self,
            messages: &[ProviderMessage],
            _tools: &[crate::agent::tools::ToolSpec],
            tx: Sender<ProviderEvent>,
        ) {
            let has_tool_result = messages
                .iter()
                .any(|message| matches!(message, ProviderMessage::ToolResult { .. }));

            if has_tool_result {
                let _ = tx.send(ProviderEvent::TurnComplete);
                return;
            }

            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-1".into(),
                name: "read_file".into(),
                arguments: serde_json::json!({
                    "path": "😀".repeat(20)
                })
                .to_string(),
            });
            let _ = tx.send(ProviderEvent::TurnComplete);
        }
    }

    struct UnicodeOutputProvider;

    impl LlmProvider for UnicodeOutputProvider {
        fn stream_turn(
            &self,
            messages: &[ProviderMessage],
            _tools: &[crate::agent::tools::ToolSpec],
            tx: Sender<ProviderEvent>,
        ) {
            let has_tool_result = messages
                .iter()
                .any(|message| matches!(message, ProviderMessage::ToolResult { .. }));

            if has_tool_result {
                let _ = tx.send(ProviderEvent::TurnComplete);
                return;
            }

            let _ = tx.send(ProviderEvent::ToolCall {
                call_id: "call-1".into(),
                name: "bash".into(),
                arguments: serde_json::json!({
                    "command": "printf '\\U1F600%.0s' {1..20}"
                })
                .to_string(),
            });
            let _ = tx.send(ProviderEvent::TurnComplete);
        }
    }

    struct CapturePromptProvider {
        prompt: Arc<Mutex<Option<String>>>,
    }

    impl LlmProvider for CapturePromptProvider {
        fn stream_turn(
            &self,
            messages: &[ProviderMessage],
            _tools: &[crate::agent::tools::ToolSpec],
            tx: Sender<ProviderEvent>,
        ) {
            let prompt = messages
                .iter()
                .rev()
                .find_map(|message| match message {
                    ProviderMessage::User { content } => Some(content.clone()),
                    _ => None,
                })
                .expect("provider should receive the user prompt");

            *self.prompt.lock().expect("prompt mutex poisoned") = Some(prompt);
            let _ = tx.send(ProviderEvent::TurnComplete);
        }
    }

    #[test]
    fn tick_handles_multibyte_tool_arguments_without_panicking() {
        let repo_path = env!("CARGO_MANIFEST_DIR");
        let mut app = ChatApp::new_for_test(repo_path);
        app.agent_runtime =
            AgentRuntime::new(Box::new(UnicodeArgumentProvider), PathBuf::from(repo_path));

        app.agent_runtime.submit_turn("read unicode path").unwrap();
        std::thread::sleep(Duration::from_millis(100));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.tick()));

        assert!(
            result.is_ok(),
            "tick should not panic on multibyte tool arguments"
        );
    }

    #[test]
    fn tick_handles_multibyte_tool_output_without_panicking() {
        let repo_path = env!("CARGO_MANIFEST_DIR");
        let mut app = ChatApp::new_for_test(repo_path);
        app.agent_runtime =
            AgentRuntime::new(Box::new(UnicodeOutputProvider), PathBuf::from(repo_path));

        app.agent_runtime
            .submit_turn("render unicode output")
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.tick()));

        assert!(
            result.is_ok(),
            "tick should not panic on multibyte tool output"
        );
    }

    #[test]
    fn send_message_queues_when_agent_busy() {
        let repo_path = env!("CARGO_MANIFEST_DIR");
        let mut app = ChatApp::new_for_test(repo_path);

        // First message dispatches normally
        app.send_message("first message");
        assert!(app.pending_messages.is_empty());

        // Simulate agent becoming busy
        app.apply_agent_event_for_test(AgentEvent::TurnStarted {
            turn_id: "t1".into(),
        });
        assert!(app.agent_state.is_busy());

        // Second and third messages should be queued, not dropped
        app.send_message("second message");
        app.send_message("third message");
        assert_eq!(app.pending_messages.len(), 2);
        assert_eq!(app.pending_messages[0].user_text, "second message");
        assert_eq!(app.pending_messages[1].user_text, "third message");

        // Both still appear in timeline (UI not lost)
        let user_messages: Vec<&str> = app
            .timeline
            .iter()
            .filter(|e| e.actor == Actor::User)
            .map(|e| e.body.as_str())
            .collect();
        assert!(user_messages.contains(&"second message"));
        assert!(user_messages.contains(&"third message"));
    }

    #[test]
    fn dispatch_next_queued_drains_fifo_on_turn_complete() {
        let repo_path = env!("CARGO_MANIFEST_DIR");
        let mut app = ChatApp::new_for_test(repo_path);

        // Queue two messages
        app.pending_messages
            .push_back(build_pending_turn("queued-a"));
        app.pending_messages
            .push_back(build_pending_turn("queued-b"));

        // Simulate TurnComplete — should dispatch first queued message
        app.apply_agent_event_for_test(AgentEvent::TurnComplete);
        assert_eq!(app.pending_messages.len(), 1);
        assert_eq!(app.pending_messages[0].user_text, "queued-b");
    }

    #[test]
    fn dispatch_next_queued_drains_on_turn_failed() {
        let repo_path = env!("CARGO_MANIFEST_DIR");
        let mut app = ChatApp::new_for_test(repo_path);

        app.pending_messages
            .push_back(build_pending_turn("queued-after-fail"));

        // TurnFailed should also drain the queue
        app.apply_agent_event_for_test(AgentEvent::TurnFailed {
            error: "test error".into(),
        });
        assert!(app.pending_messages.is_empty());
    }

    #[test]
    fn paste_action_inserts_into_composer_without_submitting() {
        let repo_path = env!("CARGO_MANIFEST_DIR");
        let mut app = ChatApp::new_for_test(repo_path);

        let pasted = "line one\nline two\nline three";
        app.handle_action(InputAction::Paste(pasted.to_owned()));

        let text = app.composer_text();
        assert!(text.contains("line one"));
        assert!(text.contains("line two"));
        assert!(text.contains("line three"));
        // No message was sent — timeline should only have the welcome message
        let user_msgs: Vec<_> = app
            .timeline
            .iter()
            .filter(|e| e.actor == Actor::User)
            .collect();
        assert!(user_msgs.is_empty(), "paste should not auto-submit");
    }

    #[test]
    fn plan_command_rewrites_prompt_for_planning_mode() {
        let repo_path = env!("CARGO_MANIFEST_DIR");
        let mut app = ChatApp::new_for_test(repo_path);
        let captured_prompt = Arc::new(Mutex::new(None));
        app.agent_runtime = AgentRuntime::new(
            Box::new(CapturePromptProvider {
                prompt: captured_prompt.clone(),
            }),
            PathBuf::from(repo_path),
        );

        app.send_message("/plan add minimax provider");
        std::thread::sleep(Duration::from_millis(50));

        let prompt = captured_prompt
            .lock()
            .expect("prompt mutex poisoned")
            .clone()
            .expect("provider should capture a prompt");

        assert!(prompt.contains("planning mode"));
        assert!(prompt.contains("First line must be a short plain-text title"));
        assert!(prompt.contains("add minimax provider"));
    }

    #[test]
    fn planning_turn_uses_thinking_while_streaming_then_sets_title() {
        let repo_path = env!("CARGO_MANIFEST_DIR");
        let mut app = ChatApp::new_for_test(repo_path);

        app.send_message("/plan add minimax provider");
        app.apply_agent_event_for_test(AgentEvent::TurnStarted {
            turn_id: "plan-1".into(),
        });

        let streaming_lines = render_to_lines_for_test(&mut app, 90, 24);
        let streaming_text = streaming_lines.join("\n");
        assert!(
            streaming_text.contains("thinking"),
            "planning turns should show thinking while streaming"
        );

        app.apply_agent_event_for_test(AgentEvent::TextDelta {
            text: "MiniMax Provider Integration\n\n1. Review current provider abstraction\n2. Add provider config\n".into(),
        });
        app.apply_agent_event_for_test(AgentEvent::TurnComplete);

        let assistant_entry = app
            .timeline
            .iter()
            .rev()
            .find(|entry| entry.actor == Actor::Assistant && entry.kind == EntryKind::Message)
            .expect("assistant response entry should exist");

        assert_eq!(
            assistant_entry.title.as_deref(),
            Some("MiniMax Provider Integration")
        );
        assert_eq!(
            assistant_entry.body,
            "1. Review current provider abstraction\n2. Add provider config"
        );

        let completed_lines = render_to_lines_for_test(&mut app, 90, 24);
        let completed_text = completed_lines.join("\n");
        assert!(completed_text.contains("MiniMax Provider Integration"));
        assert!(!completed_text.contains("Blazar #2"));
    }

    #[test]
    fn follow_up_turn_reuses_latest_plan_title_while_streaming() {
        let repo_path = env!("CARGO_MANIFEST_DIR");
        let mut app = ChatApp::new_for_test(repo_path);
        app.timeline.push(
            TimelineEntry::response("1. Review current provider abstraction")
                .with_title("MiniMax Provider Integration"),
        );
        app.active_turn_kind = Some(TurnKind::Chat);
        app.active_turn_title = Some("MiniMax Provider Integration".into());
        app.apply_agent_event_for_test(AgentEvent::TurnStarted {
            turn_id: "exec-1".into(),
        });

        let lines = render_to_lines_for_test(&mut app, 90, 24);
        let text = lines.join("\n");
        assert!(text.contains("MiniMax Provider Integration"));
        assert!(!text.contains("streaming…"));
    }
}
