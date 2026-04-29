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
                self.current_turn_timeline_start = Some(self.timeline.len());
                self.current_turn_structured_response_raw = None;
                self.current_turn_structured_entry_index = None;
                self.current_turn_contract_delta = None;
                self.current_turn_contract_delta_seen = false;
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
                if self.current_turn_contract_delta_seen && looks_like_contract_markup(&text) {
                    self.scroll_offset = u16::MAX;
                    return;
                }
                let structured_stream = self.current_turn_structured_response_raw.is_some()
                    || looks_like_contract_markup(&text);
                if structured_stream {
                    let entry_index = self.ensure_structured_stream_entry();

                    let parsed_contract = {
                        let raw = self
                            .current_turn_structured_response_raw
                            .get_or_insert_with(String::new);
                        raw.push_str(&text);
                        parse_assistant_response_contract(raw)
                            .or_else(|| parse_partial_assistant_response_contract(raw))
                    };
                    if let Some(contract) = parsed_contract {
                        let _ = self.apply_structured_contract(contract);
                    } else if let Some(entry) = self.timeline.get_mut(entry_index)
                        && entry.actor == Actor::Assistant
                        && entry.kind == EntryKind::Message
                        && entry.body.trim().is_empty()
                    {
                        entry.body = "…".to_owned();
                    }
                    self.scroll_offset = u16::MAX;
                    return;
                }

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
            AgentEvent::AssistantContractDelta { delta } => {
                self.current_turn_contract_delta_seen = true;
                let _ = self.ensure_structured_stream_entry();
                merge_assistant_contract_delta(&mut self.current_turn_contract_delta, delta);
                if let Some(contract) = self
                    .current_turn_contract_delta
                    .as_ref()
                    .map(contract_from_structured_delta)
                {
                    let _ = self.apply_structured_contract(contract);
                } else {
                    let entry_index = self.ensure_structured_stream_entry();
                    if let Some(entry) = self.timeline.get_mut(entry_index)
                        && entry.actor == Actor::Assistant
                        && entry.kind == EntryKind::Message
                        && entry.body.trim().is_empty()
                    {
                        entry.body = "…".to_owned();
                    }
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
                let error_kind =
                    is_error.then_some(runtime_error_kind_label(RuntimeErrorKind::ToolExecution));
                let message =
                    format!("chat tool call completed call_id={call_id} is_error={is_error}");
                let debug_event = self.debug_recorder.record_event(
                    "tool_call_completed",
                    completed_tool_name.as_deref(),
                    Some(&call_id),
                    error_kind,
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
                let normalized_structured_response = self
                    .finalize_side_channel_structured_response()
                    || self.finalize_structured_response();
                if !normalized_structured_response {
                    self.sanitize_internal_contract_markup();
                    if self.active_turn_kind == Some(TurnKind::Plan) {
                        self.finalize_plan_response();
                    } else if self.active_turn_kind == Some(TurnKind::Compact) {
                        self.finalize_compact_response();
                    }
                }
                self.thinking_action_name = None;
                self.current_turn_has_thinking_entry = false;
                self.current_turn_timeline_start = None;
                self.current_turn_structured_response_raw = None;
                self.current_turn_structured_entry_index = None;
                self.current_turn_contract_delta = None;
                self.current_turn_contract_delta_seen = false;
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
                self.current_turn_timeline_start = None;
                self.current_turn_structured_response_raw = None;
                self.current_turn_structured_entry_index = None;
                self.current_turn_contract_delta = None;
                self.current_turn_contract_delta_seen = false;
                self.active_turn_kind = None;
                self.active_turn_title = None;
                self.dispatch_next_queued();
                self.scroll_offset = u16::MAX;
            }
        }
    }

    fn finalize_structured_response(&mut self) -> bool {
        let contract = self
            .current_turn_structured_response_raw
            .as_deref()
            .and_then(parse_assistant_response_contract)
            .or_else(|| {
                let assistant_indices = self.current_turn_assistant_message_indices();
                if assistant_indices.is_empty() {
                    return None;
                }
                let combined_response = assistant_indices
                    .iter()
                    .filter_map(|index| self.timeline.get(*index))
                    .map(|entry| entry.body.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                parse_assistant_response_contract(&combined_response)
            });

        match contract {
            Some(contract) => self.apply_structured_contract(contract),
            None => false,
        }
    }

    fn finalize_side_channel_structured_response(&mut self) -> bool {
        let Some(contract_delta) = self.current_turn_contract_delta.as_ref() else {
            return false;
        };
        self.apply_structured_contract(contract_from_structured_delta(contract_delta))
    }

    fn apply_structured_contract(&mut self, contract: AssistantResponseContract) -> bool {
        let assistant_indices = self.current_turn_assistant_message_indices();
        if assistant_indices.is_empty() {
            return false;
        }
        let target_index = *assistant_indices
            .last()
            .expect("assistant_indices must be non-empty");
        let Some(entry) = self.timeline.get_mut(target_index) else {
            return false;
        };

        let summary = contract.summary.trim();
        let tool_summary = contract.tool_summary.trim();
        let nextstep = contract.nextstep.trim();
        let question = contract.question.trim();
        let error = contract.error.trim();
        let mut sections = Vec::new();

        match self.active_turn_kind {
            Some(TurnKind::Plan) => {
                if !summary.is_empty() {
                    entry.title = Some(summary.to_owned());
                }
                if !nextstep.is_empty() {
                    sections.push(nextstep.to_owned());
                }
                if !tool_summary.is_empty() {
                    sections.push(format!("Tool summary: {tool_summary}"));
                }
                if contract.needs_user_input && !question.is_empty() {
                    sections.push(format!("Question: {question}"));
                }
                if sections.is_empty() && !summary.is_empty() {
                    sections.push(summary.to_owned());
                }
                self.thinking_action_name = short_action_name_from_text(nextstep)
                    .or_else(|| short_action_name_from_text(summary));
            }
            Some(TurnKind::Compact) => {
                if !summary.is_empty() {
                    entry.title = Some(format!("📦 {summary}"));
                    sections.push(summary.to_owned());
                }
                if !tool_summary.is_empty() {
                    sections.push(format!("Tool summary: {tool_summary}"));
                }
                if !nextstep.is_empty() {
                    sections.push(format!("Next step: {nextstep}"));
                }
            }
            _ => {
                if !summary.is_empty() {
                    sections.push(summary.to_owned());
                }
                if !tool_summary.is_empty() {
                    sections.push(format!("Tool summary: {tool_summary}"));
                }
                if !nextstep.is_empty() {
                    sections.push(format!("Next step: {nextstep}"));
                }
                if contract.needs_user_input && !question.is_empty() {
                    sections.push(format!("Question: {question}"));
                }
                if contract.status == "blocked" && !error.is_empty() {
                    sections.push(format!("Blocked reason: {error}"));
                } else if contract.status == "failed" && !error.is_empty() {
                    sections.push(format!("Error: {error}"));
                }
            }
        }

        if sections.is_empty() {
            sections.push("…".to_owned());
        }

        entry.body = sections.join("\n\n");
        entry.details = format_structured_response_details(&contract);
        for index in assistant_indices.into_iter().rev() {
            if index != target_index {
                self.timeline.remove(index);
            }
        }
        self.current_turn_structured_entry_index = self
            .current_turn_assistant_message_indices()
            .last()
            .copied();
        true
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

    fn current_turn_assistant_message_indices(&self) -> Vec<usize> {
        if let Some(start) = self.current_turn_timeline_start {
            return self
                .timeline
                .iter()
                .enumerate()
                .skip(start.min(self.timeline.len()))
                .filter_map(|(index, entry)| {
                    (entry.actor == Actor::Assistant && entry.kind == EntryKind::Message)
                        .then_some(index)
                })
                .collect();
        }

        self.timeline
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, entry)| {
                (entry.actor == Actor::Assistant && entry.kind == EntryKind::Message)
                    .then_some(vec![index])
            })
            .unwrap_or_default()
    }

    fn ensure_structured_stream_entry(&mut self) -> usize {
        let existing = self.current_turn_structured_entry_index.and_then(|index| {
            self.timeline.get(index).and_then(|entry| {
                (entry.actor == Actor::Assistant && entry.kind == EntryKind::Message)
                    .then_some(index)
            })
        });
        if let Some(index) = existing {
            return index;
        }

        self.timeline.push(TimelineEntry::response(""));
        let index = self.timeline.len().saturating_sub(1);
        self.current_turn_structured_entry_index = Some(index);
        index
    }

    fn sanitize_internal_contract_markup(&mut self) {
        let assistant_indices = self.current_turn_assistant_message_indices();
        if assistant_indices.is_empty() {
            return;
        }

        let fallback_from_raw = self
            .current_turn_structured_response_raw
            .as_deref()
            .map(strip_contract_markup)
            .filter(|text| !text.trim().is_empty());
        let fallback_from_entries = assistant_indices
            .iter()
            .filter_map(|index| self.timeline.get(*index))
            .map(|entry| strip_contract_markup(&entry.body))
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let sanitized = fallback_from_raw.or_else(|| {
            (!fallback_from_entries.trim().is_empty()).then_some(fallback_from_entries)
        });
        let Some(sanitized) = sanitized else {
            return;
        };

        let target_index = *assistant_indices
            .last()
            .expect("assistant_indices must be non-empty");
        if let Some(entry) = self.timeline.get_mut(target_index) {
            entry.body = sanitized;
        }
        for index in assistant_indices.into_iter().rev() {
            if index != target_index {
                self.timeline.remove(index);
            }
        }
        self.current_turn_structured_entry_index = self
            .current_turn_assistant_message_indices()
            .last()
            .copied();
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

fn merge_assistant_contract_delta(
    aggregate: &mut Option<crate::agent::protocol::AssistantContractDelta>,
    delta: crate::agent::protocol::AssistantContractDelta,
) {
    if let Some(current) = aggregate.as_mut() {
        if delta.intent.is_some() {
            current.intent = delta.intent;
        }
        if delta.summary.is_some() {
            current.summary = delta.summary;
        }
        if delta.tool_summary.is_some() {
            current.tool_summary = delta.tool_summary;
        }
        if delta.nextstep.is_some() {
            current.nextstep = delta.nextstep;
        }
        if delta.needs_user_input.is_some() {
            current.needs_user_input = delta.needs_user_input;
        }
        if delta.question.is_some() {
            current.question = delta.question;
        }
        if delta.status.is_some() {
            current.status = delta.status;
        }
        if delta.error.is_some() {
            current.error = delta.error;
        }
        current.complete = current.complete || delta.complete;
    } else {
        *aggregate = Some(delta);
    }
}

fn contract_from_structured_delta(
    delta: &crate::agent::protocol::AssistantContractDelta,
) -> AssistantResponseContract {
    AssistantResponseContract {
        intent: delta.intent.clone().unwrap_or_else(|| "execute".to_owned()),
        summary: delta.summary.clone().unwrap_or_default(),
        tool_summary: delta.tool_summary.clone().unwrap_or_default(),
        nextstep: delta.nextstep.clone().unwrap_or_default(),
        needs_user_input: delta.needs_user_input.unwrap_or(false),
        question: delta.question.clone().unwrap_or_default(),
        status: delta.status.clone().unwrap_or_else(|| "ok".to_owned()),
        error: delta.error.clone().unwrap_or_default(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AssistantResponseContract {
    intent: String,
    summary: String,
    tool_summary: String,
    nextstep: String,
    needs_user_input: bool,
    question: String,
    status: String,
    error: String,
}

fn parse_assistant_response_contract(payload: &str) -> Option<AssistantResponseContract> {
    if let Some(contract) = parse_assistant_response_contract_block(payload.trim()) {
        return Some(contract);
    }
    if let Some(contract) = parse_assistant_response_contract_block_lenient(payload.trim()) {
        return Some(contract);
    }

    const OPEN: &str = "<assistant_response>";
    const CLOSE: &str = "</assistant_response>";
    let starts = payload
        .match_indices(OPEN)
        .map(|(start, _)| start)
        .collect::<Vec<_>>();
    for start in starts.into_iter().rev() {
        let candidate_start = &payload[start..];
        let Some(close_index) = candidate_start.find(CLOSE) else {
            if let Some(contract) = parse_assistant_response_contract_block_lenient(candidate_start)
            {
                return Some(contract);
            }
            continue;
        };
        let candidate = &candidate_start[..close_index + CLOSE.len()];
        if let Some(contract) = parse_assistant_response_contract_block(candidate.trim()) {
            return Some(contract);
        }
        if let Some(contract) = parse_assistant_response_contract_block_lenient(candidate.trim()) {
            return Some(contract);
        }
    }

    None
}

fn parse_assistant_response_contract_block(payload: &str) -> Option<AssistantResponseContract> {
    let trimmed = payload.trim();
    let inner = trimmed
        .strip_prefix("<assistant_response>")?
        .strip_suffix("</assistant_response>")?;

    let (intent, rest) = consume_contract_tag(inner, "intent")?;
    let (summary, rest) = consume_contract_tag(rest, "summary")?;
    let (tool_summary, rest) = consume_contract_tag(rest, "tool_summary")?;
    let (nextstep, rest) = consume_contract_tag(rest, "nextstep")?;
    let (needs_user_input_raw, rest) = consume_contract_tag(rest, "needs_user_input")?;
    let (question, rest) = consume_contract_tag(rest, "question")?;
    let (status, rest) = consume_contract_tag(rest, "status")?;
    let (error, rest) = consume_contract_tag(rest, "error")?;

    if !rest.trim().is_empty() {
        return None;
    }

    let needs_user_input = match needs_user_input_raw.trim() {
        "true" => true,
        "false" => false,
        _ => return None,
    };

    let status = status.trim();
    if !matches!(status, "ok" | "blocked" | "failed") {
        return None;
    }

    Some(AssistantResponseContract {
        intent: intent.trim().to_owned(),
        summary,
        tool_summary,
        nextstep,
        needs_user_input,
        question,
        status: status.to_owned(),
        error,
    })
}

fn parse_partial_assistant_response_contract(payload: &str) -> Option<AssistantResponseContract> {
    let candidate = latest_contract_candidate(payload)?;

    let intent =
        extract_streaming_tag_value(candidate, "intent").unwrap_or_else(|| "report".to_owned());
    let summary = extract_streaming_tag_value(candidate, "summary").unwrap_or_default();
    let tool_summary = extract_streaming_tag_value(candidate, "tool_summary").unwrap_or_default();
    let nextstep = extract_streaming_tag_value(candidate, "nextstep").unwrap_or_default();
    let question = extract_streaming_tag_value(candidate, "question").unwrap_or_default();
    let error = extract_streaming_tag_value(candidate, "error").unwrap_or_default();

    if summary.trim().is_empty()
        && tool_summary.trim().is_empty()
        && nextstep.trim().is_empty()
        && question.trim().is_empty()
        && error.trim().is_empty()
    {
        return None;
    }

    let needs_user_input = extract_streaming_tag_value(candidate, "needs_user_input")
        .map(|value| value.trim_start().starts_with("true"))
        .unwrap_or(false);

    let status = extract_streaming_tag_value(candidate, "status")
        .map(|value| {
            let lowered = value.trim().to_ascii_lowercase();
            if lowered.starts_with("blocked") {
                "blocked".to_owned()
            } else if lowered.starts_with("failed") {
                "failed".to_owned()
            } else if lowered.starts_with("ok") {
                "ok".to_owned()
            } else if needs_user_input {
                "blocked".to_owned()
            } else {
                "ok".to_owned()
            }
        })
        .unwrap_or_else(|| {
            if needs_user_input {
                "blocked".to_owned()
            } else {
                "ok".to_owned()
            }
        });

    Some(AssistantResponseContract {
        intent: intent.trim().to_owned(),
        summary,
        tool_summary,
        nextstep,
        needs_user_input,
        question,
        status,
        error,
    })
}

fn parse_assistant_response_contract_block_lenient(
    payload: &str,
) -> Option<AssistantResponseContract> {
    let trimmed = payload.trim();
    let inner = trimmed.strip_prefix("<assistant_response>")?;
    let inner = inner
        .strip_suffix("</assistant_response>")
        .unwrap_or(inner)
        .trim();

    let intent = find_contract_tag(inner, "intent").unwrap_or_else(|| "report".to_owned());
    let summary = find_contract_tag(inner, "summary").unwrap_or_default();
    let tool_summary = find_contract_tag(inner, "tool_summary").unwrap_or_default();
    let nextstep = find_contract_tag(inner, "nextstep").unwrap_or_default();
    let needs_user_input_raw =
        find_contract_tag(inner, "needs_user_input").unwrap_or_else(|| "false".to_owned());
    let question = find_contract_tag(inner, "question").unwrap_or_default();
    let status_raw = find_contract_tag(inner, "status").unwrap_or_default();
    let error = find_contract_tag(inner, "error").unwrap_or_default();

    if summary.trim().is_empty()
        && tool_summary.trim().is_empty()
        && nextstep.trim().is_empty()
        && question.trim().is_empty()
        && error.trim().is_empty()
    {
        return None;
    }

    let needs_user_input = needs_user_input_raw.trim().eq_ignore_ascii_case("true");
    let status = match status_raw.trim() {
        "ok" | "blocked" | "failed" => status_raw.trim().to_owned(),
        _ if needs_user_input => "blocked".to_owned(),
        _ => "ok".to_owned(),
    };

    Some(AssistantResponseContract {
        intent: intent.trim().to_owned(),
        summary,
        tool_summary,
        nextstep,
        needs_user_input,
        question,
        status,
        error,
    })
}

fn consume_contract_tag<'a>(rest: &'a str, tag: &str) -> Option<(String, &'a str)> {
    let working = rest.trim_start();
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let after_open = working.strip_prefix(&open)?;
    let close_index = after_open.find(&close)?;
    let value = after_open[..close_index].to_owned();
    let remaining = &after_open[close_index + close.len()..];
    Some((value, remaining))
}

fn latest_contract_candidate(payload: &str) -> Option<&str> {
    let start = payload.rfind("<assistant_response")?;
    let candidate = &payload[start..];
    if let Some(pos) = candidate.find("<assistant_response>") {
        return Some(&candidate[pos..]);
    }
    Some(candidate)
}

fn extract_streaming_tag_value(payload: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = payload.find(&open)? + open.len();
    let remaining = &payload[start..];
    let value = match remaining.find(&close) {
        Some(end) => &remaining[..end],
        None => remaining,
    };
    Some(value.trim().to_owned())
}

fn find_contract_tag(payload: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = payload.find(&open)? + open.len();
    let end = payload[start..].find(&close)? + start;
    Some(payload[start..end].to_owned())
}

fn format_structured_response_details(contract: &AssistantResponseContract) -> String {
    let mut lines = vec![
        format!("intent={}", contract.intent.trim()),
        format!("status={}", contract.status.trim()),
        format!("needs_user_input={}", contract.needs_user_input),
    ];
    if !contract.error.trim().is_empty() {
        lines.push(format!("error={}", contract.error.trim()));
    }
    lines.join("\n")
}

fn looks_like_contract_markup(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    [
        "<assistant_",
        "</assistant_",
        "<intent",
        "<summary",
        "<tool_summary",
        "<nextstep",
        "<needs_user_input",
        "<question",
        "<status",
        "<error",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn strip_contract_markup(text: &str) -> String {
    let mut stripped = String::new();
    let mut inside_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' if inside_tag => {
                inside_tag = false;
                if !stripped.ends_with('\n') {
                    stripped.push('\n');
                }
            }
            _ if !inside_tag => stripped.push(ch),
            _ => {}
        }
    }

    stripped
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
