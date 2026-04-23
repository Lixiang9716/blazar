use super::turns::{
    extract_plan_title_and_body, extract_tool_call_metadata_line, format_tool_call_details,
};
use super::*;
use crate::agent::runtime::RuntimeErrorKind;
use crate::observability::logging::emit_structured_event;

impl ChatApp {
    #[doc(hidden)]
    pub fn apply_agent_event_for_test(&mut self, event: AgentEvent) {
        self.apply_agent_event(event);
    }

    pub(super) fn apply_agent_event(&mut self, event: AgentEvent) {
        let turn_id_context = self.current_turn_id().map(str::to_owned);
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
                batch_id,
                replay_index,
                normalized_claims,
            } => {
                debug!(
                    "tick: ToolCallStarted call_id={} tool={} arguments_len={}",
                    call_id,
                    tool_name,
                    arguments.len()
                );
                let message = format!(
                    "chat tool call started call_id={call_id} tool={tool_name} arguments_len={}",
                    arguments.len()
                );
                emit_structured_event(
                    log::Level::Debug,
                    module_path!(),
                    "tool_call_started",
                    &message,
                    None,
                    self.current_turn_id(),
                    Some(&tool_name),
                    None,
                    None,
                );
                self.timeline.push(TimelineEntry::tool_call(
                    call_id,
                    tool_name,
                    kind,
                    summarize_tool_arguments(&arguments),
                    format_tool_call_details(
                        &arguments,
                        batch_id,
                        replay_index,
                        &normalized_claims,
                    ),
                    ToolCallStatus::Running,
                ));
                self.scroll_offset = u16::MAX;
            }
            AgentEvent::ToolCallCompleted {
                call_id,
                output,
                is_error,
            } => {
                let completed_tool_name = self.timeline.iter().rev().find_map(|entry| match &entry
                    .kind
                {
                    EntryKind::ToolCall {
                        call_id: existing,
                        tool_name,
                        ..
                    } if existing == &call_id => Some(tool_name.clone()),
                    _ => None,
                });
                debug!(
                    "tick: ToolCallCompleted call_id={} is_error={} output_len={}",
                    call_id,
                    is_error,
                    output.len()
                );
                let message =
                    format!("chat tool call completed call_id={call_id} is_error={is_error}");
                emit_structured_event(
                    log::Level::Debug,
                    module_path!(),
                    "tool_call_completed",
                    &message,
                    None,
                    self.current_turn_id(),
                    completed_tool_name.as_deref(),
                    None,
                    None,
                );
                if let Some(entry) = self.timeline.iter_mut().rev().find(|entry| {
                    matches!(
                        &entry.kind,
                        EntryKind::ToolCall { call_id: existing, .. } if existing == &call_id
                    )
                }) {
                    entry.body = summarize_tool_output(&output);
                    entry.details = match extract_tool_call_metadata_line(&entry.details) {
                        Some(metadata_line) if output.is_empty() => metadata_line,
                        Some(metadata_line) => format!("{output}\n\n{metadata_line}"),
                        None => output,
                    };
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
                self.dispatch_next_queued();
                self.scroll_offset = u16::MAX;
            }
            AgentEvent::AcpAgentsRefreshFailed { error } => {
                warn!("tick: AcpAgentsRefreshFailed error={error}");
                self.timeline.push(TimelineEntry::warning(format!(
                    "ACP agent discovery failed: {error}"
                )));
                self.dispatch_next_queued();
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
                    let safe_log_message =
                        turn_failed_app_log_message(kind, turn_id_context.as_deref());
                    warn!("{safe_log_message}");
                    emit_structured_event(
                        log::Level::Warn,
                        module_path!(),
                        "turn_failed",
                        &safe_log_message,
                        None,
                        turn_id_context.as_deref(),
                        None,
                        None,
                        Some(runtime_error_kind_label(kind)),
                    );
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

    fn current_turn_id(&self) -> Option<&str> {
        match &self.agent_state.turn_state {
            crate::agent::state::TurnState::Streaming { turn_id } => Some(turn_id.as_str()),
            _ => None,
        }
    }
}

pub(super) fn turn_failed_app_log_message(kind: RuntimeErrorKind, turn_id: Option<&str>) -> String {
    match turn_id {
        Some(turn_id) => format!(
            "tick: TurnFailed kind={} turn_id={turn_id} details=redacted",
            runtime_error_kind_label(kind)
        ),
        None => format!(
            "tick: TurnFailed kind={} details=redacted",
            runtime_error_kind_label(kind)
        ),
    }
}

fn runtime_error_kind_label(kind: RuntimeErrorKind) -> &'static str {
    match kind {
        RuntimeErrorKind::ProviderTransient => "ProviderTransient",
        RuntimeErrorKind::ProviderFatal => "ProviderFatal",
        RuntimeErrorKind::ProtocolInvalidPayload => "ProtocolInvalidPayload",
        RuntimeErrorKind::ToolExecution => "ToolExecution",
        RuntimeErrorKind::Cancelled => "Cancelled",
    }
}
