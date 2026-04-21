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
    pending_messages: VecDeque<String>,
    /// Workspace root for recreating the provider on model switch.
    workspace_root: PathBuf,
    /// Display name of the active model (e.g. "Qwen/Qwen3-8B").
    model_name: String,
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
        }
    }

    pub fn new_for_test(_repo_path: &str) -> Self {
        let mut app = Self::new(_repo_path);
        // Use a fixed display path so snapshots are environment-independent.
        app.display_path = "~/blazar".to_owned();
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
            TurnState::Streaming { .. } => "streaming…".to_owned(),
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
            body: trimmed.to_owned(),
        });

        // Add user message to timeline
        self.timeline.push(TimelineEntry::user_message(trimmed));

        // Admission control: queue if agent is busy instead of dropping
        if self.agent_state.is_busy() {
            info!(
                "send_message: queued (agent busy) queue_depth={}",
                self.pending_messages.len() + 1
            );
            self.pending_messages.push_back(trimmed.to_owned());
            return;
        }

        // Dispatch to agent runtime — response arrives via events in tick()
        if let Err(e) = self.agent_runtime.submit_turn(trimmed) {
            warn!("send_message: submit_turn failed: {e}");
            self.timeline
                .push(TimelineEntry::warning(format!("Runtime error: {e}")));
        }

        // Auto-scroll to bottom
        self.scroll_offset = u16::MAX;
    }

    /// Dispatches the next queued message to the agent runtime (FIFO).
    /// Called after any terminal turn event (TurnComplete, TurnFailed).
    fn dispatch_next_queued(&mut self) {
        if let Some(msg) = self.pending_messages.pop_front() {
            info!(
                "dispatch_next_queued: dispatching len={} remaining={}",
                msg.len(),
                self.pending_messages.len()
            );
            if let Err(e) = self.agent_runtime.submit_turn(&msg) {
                warn!("dispatch_next_queued: submit_turn failed: {e}");
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
                self.dispatch_next_queued();
                self.scroll_offset = u16::MAX;
            }
        }
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

/// Detect the current git branch. Returns "main" as fallback.
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
        .unwrap_or_else(|| "main".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ProviderEvent, ProviderMessage};
    use std::path::PathBuf;
    use std::sync::mpsc::Sender;
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
        assert_eq!(app.pending_messages[0], "second message");
        assert_eq!(app.pending_messages[1], "third message");

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
        app.pending_messages.push_back("queued-a".into());
        app.pending_messages.push_back("queued-b".into());

        // Simulate TurnComplete — should dispatch first queued message
        app.apply_agent_event_for_test(AgentEvent::TurnComplete);
        assert_eq!(app.pending_messages.len(), 1);
        assert_eq!(app.pending_messages[0], "queued-b");
    }

    #[test]
    fn dispatch_next_queued_drains_on_turn_failed() {
        let repo_path = env!("CARGO_MANIFEST_DIR");
        let mut app = ChatApp::new_for_test(repo_path);

        app.pending_messages.push_back("queued-after-fail".into());

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
}
