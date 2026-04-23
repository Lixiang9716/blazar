use super::turns::extract_plan_title_and_body;
use super::*;
use crate::agent::runtime::RuntimeErrorKind;

impl ChatApp {
    #[doc(hidden)]
    pub fn apply_agent_event_for_test(&mut self, event: AgentEvent) {
        self.apply_agent_event(event);
    }

    pub(super) fn apply_agent_event(&mut self, event: AgentEvent) {
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
                kind,
                arguments,
                ..
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
                    kind,
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
            AgentEvent::AcpAgentsRefreshed => {
                self.timeline
                    .push(TimelineEntry::hint("ACP agent discovery complete."));
                self.scroll_offset = u16::MAX;
            }
            AgentEvent::AcpAgentsRefreshFailed { error } => {
                warn!("tick: AcpAgentsRefreshFailed error={error}");
                self.timeline.push(TimelineEntry::warning(format!(
                    "ACP agent discovery failed: {error}"
                )));
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
            AgentEvent::TurnFailed { kind, error } => {
                if kind == RuntimeErrorKind::Cancelled {
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

    pub(super) fn streaming_title_for_turn(&self, kind: TurnKind) -> Option<String> {
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
