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
fn captured_structured_events_for_test() -> &'static std::sync::Mutex<Vec<String>> {
    use std::sync::{Mutex, OnceLock};

    static CAPTURED: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    CAPTURED.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(test)]
fn capture_structured_event_for_test(event: String) {
    if let Ok(mut captured) = captured_structured_events_for_test().lock() {
        captured.push(event);
    }
}

#[cfg(test)]
pub fn clear_captured_structured_events_for_test() {
    if let Ok(mut captured) = captured_structured_events_for_test().lock() {
        captured.clear();
    }
}

#[cfg(test)]
pub fn take_captured_structured_events_for_test() -> Vec<String> {
    captured_structured_events_for_test()
        .lock()
        .map(|mut captured| std::mem::take(&mut *captured))
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "logging_tests.rs"]
mod tests;
