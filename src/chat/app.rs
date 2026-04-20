use crate::agent::protocol::AgentEvent;
use crate::agent::runtime::AgentRuntime;
use crate::agent::state::{AgentRuntimeState, TurnState};

use crate::chat::input::InputAction;
use crate::chat::model::{Actor, Author, ChatMessage, EntryKind, TimelineEntry};
use crate::chat::picker::{ModalPicker, PickerItem};
use crate::chat::theme::ChatTheme;
use crate::provider::LlmProvider;
use crate::provider::echo::EchoProvider;
use crate::provider::siliconflow::SiliconFlowConfig;
use log::{debug, info, trace, warn};
use ratatui_textarea::TextArea;
use std::cell::Cell;
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
        let provider: Box<dyn LlmProvider> = match SiliconFlowConfig::load(repo_path) {
            Ok(cfg) => Box::new(crate::provider::siliconflow::SiliconFlowProvider::new(cfg)),
            Err(_) => Box::new(EchoProvider::default()),
        };

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
            agent_runtime: AgentRuntime::new(provider),
            agent_state: AgentRuntimeState::default(),
        }
    }

    pub fn new_for_test(_repo_path: &str) -> Self {
        let mut app = Self::new(_repo_path);
        // Use a fixed display path so snapshots are environment-independent.
        app.display_path = "~/blazar".to_owned();
        // Always use EchoProvider in tests — no network calls.
        app.agent_runtime = AgentRuntime::new(Box::new(EchoProvider::new(0)));
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

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        self.picker
            .overlay_state_mut()
            .tick(std::time::Duration::from_millis(100));

        // Drain agent runtime events
        while let Some(event) = self.agent_runtime.try_recv() {
            let state_changed = self.agent_state.apply_event(&event);

            match &event {
                AgentEvent::TurnStarted { .. } => {
                    debug!("tick: TurnStarted");
                    self.scroll_offset = u16::MAX;
                }
                AgentEvent::ThinkingDelta { text } => {
                    trace!("tick: ThinkingDelta len={}", text.len());
                    let needs_new = self
                        .timeline
                        .last()
                        .is_none_or(|e| e.kind != EntryKind::Thinking);
                    if needs_new {
                        self.timeline.push(TimelineEntry::thinking(""));
                    }
                    if let Some(last) = self.timeline.last_mut() {
                        last.body.push_str(text);
                        // Mirror into details so Ctrl+O can show full text
                        last.details.push_str(text);
                    }
                    self.scroll_offset = u16::MAX;
                }
                AgentEvent::TextDelta { text } => {
                    trace!("tick: TextDelta len={}", text.len());
                    // Append to the current assistant response entry; create one if
                    // the last entry is not an assistant Message (could be user msg,
                    // thinking, tool_use, etc.)
                    let needs_new = self.timeline.last().is_none_or(|e| {
                        !(e.actor == Actor::Assistant && e.kind == EntryKind::Message)
                    });
                    if needs_new {
                        self.timeline.push(TimelineEntry::response(""));
                    }
                    if let Some(last) = self.timeline.last_mut() {
                        last.body.push_str(text);
                    }
                    self.scroll_offset = u16::MAX;
                }
                AgentEvent::ToolCallRequest { payload } => {
                    debug!("tick: ToolCallRequest payload_len={}", payload.len());
                    self.timeline.push(TimelineEntry::tool_use(
                        "function_call",
                        &payload[..payload.len().min(60)],
                        0,
                        0,
                        payload.clone(),
                    ));
                    self.scroll_offset = u16::MAX;
                }
                AgentEvent::TurnComplete => {
                    debug!("tick: TurnComplete");
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
                    self.scroll_offset = u16::MAX;
                }
            }

            let _ = state_changed;
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

        // Admission control: reject if agent is already busy
        if self.agent_state.is_busy() {
            debug!("send_message: rejected — agent busy");
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
                        self.picker.close();

                        // Theme name selected (no / prefix) — apply it
                        if !cmd.starts_with('/') {
                            self.set_theme(&cmd);
                            self.picker = ModalPicker::command_palette();
                            return;
                        }

                        // /theme selected — open theme picker
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
                            self.picker = ModalPicker::new("Select Theme", theme_items);
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
            _ => {}
        }
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
