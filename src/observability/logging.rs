use flexi_logger::DeferredNow;
use log::Level;
use log::Record;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fmt::Display, io::Write};

const STRUCTURED_EVENT_PREFIX: &str = "__blazar_structured_event__:";

#[allow(clippy::too_many_arguments)]
pub fn format_event_json(
    level: &str,
    target: &str,
    event: &str,
    message: &str,
    trace_id: Option<&str>,
    turn_id: Option<&str>,
    tool_name: Option<&str>,
    agent_id: Option<&str>,
    error_kind: Option<&str>,
) -> String {
    json!({
        "ts": timestamp_seconds(),
        "level": level,
        "target": target,
        "event": event,
        "message": message,
        "trace_id": trace_id,
        "turn_id": turn_id,
        "tool_name": tool_name,
        "agent_id": agent_id,
        "error_kind": error_kind,
    })
    .to_string()
}

#[allow(clippy::too_many_arguments)]
pub fn emit_structured_event(
    level: Level,
    target: &str,
    event: &str,
    message: &str,
    trace_id: Option<&str>,
    turn_id: Option<&str>,
    tool_name: Option<&str>,
    agent_id: Option<&str>,
    error_kind: Option<&str>,
) {
    let line = format_event_json(
        &display_to_string(level),
        target,
        event,
        message,
        trace_id,
        turn_id,
        tool_name,
        agent_id,
        error_kind,
    );
    #[cfg(test)]
    capture_structured_event_for_test(line.clone());
    log::log!(target: target, level, "{STRUCTURED_EVENT_PREFIX}{line}");
}

fn timestamp_seconds() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
        .to_string()
}

pub fn flexi_structured_format(
    writer: &mut dyn Write,
    _now: &mut DeferredNow,
    record: &Record<'_>,
) -> std::io::Result<()> {
    let message = record.args().to_string();
    if let Some(line) = message.strip_prefix(STRUCTURED_EVENT_PREFIX) {
        return writeln!(writer, "{line}");
    }

    let line = format_event_json(
        &display_to_string(record.level()),
        record.target(),
        "app_log",
        &message,
        None,
        None,
        None,
        None,
        None,
    );
    writeln!(writer, "{line}")
}

fn display_to_string(value: impl Display) -> String {
    value.to_string()
}

#[cfg(test)]
thread_local! {
    static CAPTURED_STRUCTURED_EVENTS_FOR_TEST: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn capture_structured_event_for_test(event: String) {
    CAPTURED_STRUCTURED_EVENTS_FOR_TEST.with(|captured| {
        if let Some(events) = captured.borrow_mut().as_mut() {
            events.push(event);
        }
    });
}

#[cfg(test)]
struct StructuredEventCaptureGuard {
    previous: Option<Vec<String>>,
    restore_on_drop: bool,
}

#[cfg(test)]
impl StructuredEventCaptureGuard {
    fn new() -> Self {
        let previous =
            CAPTURED_STRUCTURED_EVENTS_FOR_TEST.with(|captured| captured.replace(Some(Vec::new())));
        Self {
            previous,
            restore_on_drop: true,
        }
    }

    fn finish(mut self) -> Vec<String> {
        let captured = CAPTURED_STRUCTURED_EVENTS_FOR_TEST.with(|events| {
            let mut events = events.borrow_mut();
            let captured = events.take().unwrap_or_default();
            *events = self.previous.take();
            captured
        });
        self.restore_on_drop = false;
        captured
    }
}

#[cfg(test)]
impl Drop for StructuredEventCaptureGuard {
    fn drop(&mut self) {
        if self.restore_on_drop {
            CAPTURED_STRUCTURED_EVENTS_FOR_TEST.with(|captured| {
                *captured.borrow_mut() = self.previous.take();
            });
        }
    }
}

#[cfg(test)]
pub fn with_captured_structured_events_for_test<T>(
    operation: impl FnOnce() -> T,
) -> (T, Vec<String>) {
    let capture = StructuredEventCaptureGuard::new();
    let result = operation();
    let captured = capture.finish();
    (result, captured)
}

#[cfg(test)]
#[path = "../../tests/unit/observability/logging_tests.rs"]
mod tests;
