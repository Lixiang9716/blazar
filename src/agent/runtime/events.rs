use std::sync::mpsc::Sender;

use log::Level;

use super::RuntimeErrorKind;
use crate::agent::protocol::AgentEvent;
use crate::agent::tools::ToolKind;
use crate::observability::logging::{StructuredEventContext, emit_structured_event_with_context};

pub(crate) struct ToolCallStartMetadata {
    pub(crate) batch_id: u32,
    pub(crate) replay_index: usize,
    pub(crate) normalized_claims: Vec<String>,
}

fn emit_tool_args_fim_event(
    level: Level,
    event: &str,
    message: &str,
    call_id: &str,
    tool_name: &str,
    parse_error_category: &str,
    repaired: bool,
) {
    emit_structured_event_with_context(
        level,
        module_path!(),
        event,
        message,
        StructuredEventContext {
            tool_name: Some(tool_name),
            error_kind: Some(parse_error_category),
            parse_error_category: Some(parse_error_category),
            repaired: Some(repaired),
            call_id: Some(call_id),
            ..StructuredEventContext::default()
        },
    );
}

pub(super) fn emit_tool_args_fim_correction_requested(
    call_id: &str,
    tool_name: &str,
    parse_error_category: &str,
) {
    emit_tool_args_fim_event(
        Level::Info,
        "tool_args_fim_correction_requested",
        "runtime requested bounded FIM-style tool-arg correction",
        call_id,
        tool_name,
        parse_error_category,
        false,
    );
}

pub(super) fn emit_tool_args_fim_correction_succeeded(
    call_id: &str,
    tool_name: &str,
    parse_error_category: &str,
) {
    emit_tool_args_fim_event(
        Level::Info,
        "tool_args_fim_correction_succeeded",
        "runtime accepted bounded FIM-style tool-arg correction",
        call_id,
        tool_name,
        parse_error_category,
        true,
    );
}

pub(super) fn emit_tool_args_fim_correction_failed(
    call_id: &str,
    tool_name: &str,
    parse_error_category: &str,
) {
    emit_tool_args_fim_event(
        Level::Warn,
        "tool_args_fim_correction_failed",
        "runtime rejected bounded FIM-style tool-arg correction",
        call_id,
        tool_name,
        parse_error_category,
        false,
    );
}

/// Observer that receives lifecycle events during a turn.
pub(crate) trait TurnObserver {
    fn on_text_delta(&self, text: &str);
    fn on_thinking_delta(&self, text: &str);
    fn on_tool_call_started(
        &self,
        call_id: &str,
        tool_name: &str,
        kind: ToolKind,
        arguments: &str,
        metadata: ToolCallStartMetadata,
    );
    fn on_tool_call_completed(&self, call_id: &str, output: &str, is_error: bool);
    fn on_turn_failed(&self, kind: RuntimeErrorKind, error: &str);
}

/// Observer that forwards events to a `Sender<AgentEvent>` (UI channel).
pub(crate) struct ChannelObserver<'a> {
    pub(crate) tx: &'a Sender<AgentEvent>,
}

impl TurnObserver for ChannelObserver<'_> {
    fn on_text_delta(&self, text: &str) {
        let _ = self.tx.send(AgentEvent::TextDelta {
            text: text.to_owned(),
        });
    }

    fn on_thinking_delta(&self, text: &str) {
        let _ = self.tx.send(AgentEvent::ThinkingDelta {
            text: text.to_owned(),
        });
    }

    fn on_tool_call_started(
        &self,
        call_id: &str,
        tool_name: &str,
        kind: ToolKind,
        arguments: &str,
        metadata: ToolCallStartMetadata,
    ) {
        let _ = self.tx.send(AgentEvent::ToolCallStarted {
            call_id: call_id.to_owned(),
            tool_name: tool_name.to_owned(),
            kind,
            arguments: arguments.to_owned(),
            batch_id: metadata.batch_id,
            replay_index: metadata.replay_index,
            normalized_claims: metadata.normalized_claims,
        });
    }

    fn on_tool_call_completed(&self, call_id: &str, output: &str, is_error: bool) {
        let _ = self.tx.send(AgentEvent::ToolCallCompleted {
            call_id: call_id.to_owned(),
            output: output.to_owned(),
            is_error,
        });
    }

    fn on_turn_failed(&self, kind: RuntimeErrorKind, error: &str) {
        let _ = self.tx.send(AgentEvent::TurnFailed {
            kind,
            error: error.to_owned(),
        });
    }
}

/// Observer that silently discards all events.
///
/// Used by sub-agent tool invocations where only the final text matters.
pub(crate) struct SilentObserver;

impl TurnObserver for SilentObserver {
    fn on_text_delta(&self, _text: &str) {}
    fn on_thinking_delta(&self, _text: &str) {}

    fn on_tool_call_started(
        &self,
        _call_id: &str,
        _tool_name: &str,
        _kind: ToolKind,
        _arguments: &str,
        _metadata: ToolCallStartMetadata,
    ) {
    }

    fn on_tool_call_completed(&self, _call_id: &str, _output: &str, _is_error: bool) {}
    fn on_turn_failed(&self, _kind: RuntimeErrorKind, _error: &str) {}
}
