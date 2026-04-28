use super::turns::{
    extract_plan_action_names, extract_plan_title_and_body, extract_tool_call_metadata_line,
    format_tool_call_details, parse_next_step_name_line, short_action_name_from_text,
};
use super::*;
use crate::agent::runtime::RuntimeErrorKind;
use crate::observability::debug::DebugEventSnapshot;
use crate::observability::logging::{StructuredEventContext, emit_structured_event_with_context};

impl ChatApp {
    #[doc(hidden)]
    pub fn apply_agent_event_for_test(&mut self, event: AgentEvent) {
        self.apply_agent_event(event);
    }

    #[doc(hidden)]
    pub fn set_context_usage_for_test(&mut self, used_tokens: u32, max_tokens: u32) {
        self.context_usage = Some(ContextUsage {
            used_tokens,
            max_tokens,
        });
    }

    #[doc(hidden)]
    pub fn set_model_context_max_tokens_for_test(&mut self, max_tokens: Option<u32>) {
        self.model_metadata.model_context_max_tokens = max_tokens;
    }

    #[doc(hidden)]
    pub fn set_pr_label_for_test(&mut self, pr_label: Option<String>) {
        self.git_pr_label = pr_label;
    }

    #[doc(hidden)]
    pub fn set_referenced_files_for_test(&mut self, referenced_files: Vec<String>) {
        self.referenced_files = referenced_files;
    }

    pub(super) fn apply_agent_event(&mut self, event: AgentEvent) {
        let turn_id_context = self.current_turn_id().map(str::to_owned);
        let _ = self.agent_state.apply_event(&event);

        match event {
            AgentEvent::TurnStarted { turn_id } => {
                debug!("tick: TurnStarted");
                self.thinking_action_name = None;
                self.current_turn_has_thinking_entry = false;
                self.debug_recorder.start_turn(
                    &turn_id,
                    self.active_turn_kind_label(),
                    self.pending_messages.len(),
                );
                self.refresh_active_turn_status_label();
                self.scroll_offset = u16::MAX;
            }
            AgentEvent::ThinkingDelta { text } => {
                trace!("tick: ThinkingDelta len={}", text.len());
                let tail_is_thinking = self
                    .timeline
                    .last()
                    .is_some_and(|entry| entry.kind == EntryKind::Thinking);
                let in_streaming_turn = matches!(
                    self.agent_state.turn_state,
                    crate::agent::state::TurnState::Streaming { .. }
                );
                let needs_new = if in_streaming_turn && !self.current_turn_has_thinking_entry {
                    true
                } else {
                    !tail_is_thinking
                };
                if needs_new {
                    self.timeline.push(TimelineEntry::thinking(""));
                }
                if let Some(last) = self.timeline.last_mut() {
                    last.body.push_str(&text);
                    last.details.push_str(&text);
                    self.current_turn_has_thinking_entry = true;
                    if let Some(action_name) = parse_next_step_name_line(&last.body)
                        .or_else(|| stable_fallback_action_name(&last.body))
                    {
                        self.thinking_action_name = Some(action_name);
                    }
                }
                self.refresh_active_turn_status_label();
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
            AgentEvent::UsageUpdated(crate::agent::protocol::AgentUsage {
                prompt_tokens: _,
                completion_tokens: _,
                total_tokens,
            }) => {
                let resolved_max = self.model_metadata.resolved_context_limit().unwrap_or(0);
                self.context_usage = Some(ContextUsage {
                    used_tokens: total_tokens,
                    max_tokens: resolved_max,
                });
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
                self.ingest_referenced_files_from_claims(&normalized_claims);
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
                let debug_event = self.debug_recorder.record_event(
                    "tool_call_started",
                    Some(&tool_name),
                    Some(&call_id),
                    None,
                    self.pending_messages.len(),
                    &message,
                );
                emit_structured_event_with_context(
                    log::Level::Debug,
                    module_path!(),
                    "tool_call_started",
                    &message,
                    structured_debug_context(&debug_event),
                );
                self.timeline.push(TimelineEntry::tool_call(
                    call_id,
                    tool_name,
                    kind,
                    arguments.clone(),
                    summarize_tool_arguments(&arguments),
                    append_tool_debug_details(
                        format_tool_call_details(
                            &arguments,
                            batch_id,
                            replay_index,
                            &normalized_claims,
                        ),
                        &debug_event,
                    ),
                    ToolCallStatus::Running,
                ));
                self.refresh_active_turn_status_label();
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
                let debug_event = self.debug_recorder.record_event(
                    "tool_call_completed",
                    completed_tool_name.as_deref(),
                    Some(&call_id),
                    None,
                    self.pending_messages.len(),
                    &message,
                );
                emit_structured_event_with_context(
                    log::Level::Debug,
                    module_path!(),
                    "tool_call_completed",
                    &message,
                    structured_debug_context(&debug_event),
                );
                if let Some(entry) = self.timeline.iter_mut().rev().find(|entry| {
                    matches!(
                        &entry.kind,
                        EntryKind::ToolCall { call_id: existing, .. } if existing == &call_id
                    )
                }) {
                    entry.body = summarize_tool_output(&output);
                    let details = match extract_tool_call_metadata_line(&entry.details) {
                        Some(metadata_line) if output.is_empty() => metadata_line,
                        Some(metadata_line) => format!("{output}\n\n{metadata_line}"),
                        None => output,
                    };
                    entry.details = append_tool_debug_details(details, &debug_event);
                    if let EntryKind::ToolCall { status, .. } = &mut entry.kind {
                        *status = if is_error {
                            ToolCallStatus::Error
                        } else {
                            ToolCallStatus::Success
                        };
                    }
                }
                self.refresh_active_turn_status_label();
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
                self.debug_recorder.record_event(
                    "turn_complete",
                    None,
                    None,
                    None,
                    self.pending_messages.len(),
                    "chat turn complete",
                );
                self.debug_recorder.finish_turn("completed", None, None);
                if self.active_turn_kind == Some(TurnKind::Plan) {
                    self.finalize_plan_response();
                } else if self.active_turn_kind == Some(TurnKind::Compact) {
                    self.finalize_compact_response();
                }
                self.thinking_action_name = None;
                self.current_turn_has_thinking_entry = false;
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
                    let debug_event = self.debug_recorder.record_event(
                        "turn_failed",
                        None,
                        None,
                        Some(runtime_error_kind_label(kind)),
                        self.pending_messages.len(),
                        &safe_log_message,
                    );
                    emit_structured_event_with_context(
                        log::Level::Warn,
                        module_path!(),
                        "turn_failed",
                        &safe_log_message,
                        structured_debug_context(&debug_event),
                    );
                    let details = self.debug_recorder.latest_turn_bundle().unwrap_or_default();
                    self.timeline.push(
                        TimelineEntry::warning(format!("Agent error: {error}"))
                            .with_details(details),
                    );
                    self.debug_recorder.finish_turn(
                        "failed",
                        Some(runtime_error_kind_label(kind)),
                        Some(&error),
                    );
                }
                self.thinking_action_name = None;
                self.current_turn_has_thinking_entry = false;
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

        self.thinking_action_name = extract_plan_action_names(&body).first().cloned();
        entry.title = Some(title);
        entry.body = body;
    }

    fn finalize_compact_response(&mut self) {
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

        entry.title = Some(format!("📦 {}", title));
        entry.body = body;
    }

    pub(super) fn streaming_title_for_turn(&self, kind: TurnKind) -> Option<String> {
        match kind {
            TurnKind::Plan => None,
            TurnKind::Chat => self.latest_plan_title(),
            TurnKind::Compact => Some("Compacting...".to_owned()),
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

fn stable_fallback_action_name(text: &str) -> Option<String> {
    const FILLER: &[&str] = &[
        "next", "step", "then", "will", "would", "should", "i", "we", "to", "the", "a", "an",
        "and", "now",
    ];

    let tokens = text.split_whitespace().collect::<Vec<_>>();
    for (index, raw_token) in tokens.iter().enumerate() {
        let trimmed = raw_token
            .trim_matches(|c: char| {
                matches!(
                    c,
                    ',' | '.'
                        | '!'
                        | '?'
                        | ';'
                        | ':'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '"'
                        | '\''
                        | '`'
                        | '*'
                        | '-'
                )
            })
            .trim();

        if trimmed.is_empty() || trimmed.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        let lowered = trimmed.to_ascii_lowercase();
        if FILLER.iter().any(|filler| *filler == lowered) {
            continue;
        }

        let has_boundary_after_token = index + 1 < tokens.len() || trimmed.len() != raw_token.len();
        if has_boundary_after_token {
            return short_action_name_from_text(trimmed);
        }

        return None;
    }

    None
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

fn structured_debug_context<'a>(event: &'a DebugEventSnapshot) -> StructuredEventContext<'a> {
    StructuredEventContext {
        turn_id: event.turn_id.as_deref(),
        tool_name: event.tool_name.as_deref(),
        error_kind: event.error_kind.as_deref(),
        call_id: event.call_id.as_deref(),
        session_id: Some(event.session_id.as_str()),
        workspace_path: Some(event.workspace_path.as_str()),
        queue_depth: Some(event.queue_depth),
        event_seq: event.event_seq,
        turn_kind: event.turn_kind.as_deref(),
        ..StructuredEventContext::default()
    }
}

fn append_tool_debug_details(details: String, event: &DebugEventSnapshot) -> String {
    let mut lines = Vec::new();
    if !details.is_empty() {
        lines.push(details);
    }
    if let Some(turn_id) = event.turn_id.as_deref() {
        lines.push(format!("debug.turn_id={turn_id}"));
    }
    if let Some(call_id) = event.call_id.as_deref() {
        lines.push(format!("debug.call_id={call_id}"));
    }
    if let Some(event_seq) = event.event_seq {
        lines.push(format!("debug.event_seq={event_seq}"));
    }
    lines.join("\n")
}
