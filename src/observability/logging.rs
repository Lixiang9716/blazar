use flexi_logger::DeferredNow;
use log::Record;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fmt::Display, io::Write};

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

fn timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

pub fn flexi_structured_format(
    writer: &mut dyn Write,
    _now: &mut DeferredNow,
    record: &Record<'_>,
) -> std::io::Result<()> {
    let line = format_event_json(
        &display_to_string(record.level()),
        record.target(),
        "app_log",
        &record.args().to_string(),
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
#[path = "logging_tests.rs"]
mod tests;
