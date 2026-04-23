use crate::observability::logging::format_event_json;
use serde_json::Value;

#[test]
fn structured_log_contains_required_stable_keys() {
    let raw = format_event_json(
        "INFO",
        "blazar::agent::runtime",
        "turn_failed",
        "runtime turn failed",
        Some("trace-1"),
        Some("turn-7"),
        Some("bash"),
        Some("agent-echo"),
        Some("ProviderFatal"),
    );
    let value: Value = serde_json::from_str(&raw).expect("valid json");

    for key in [
        "ts",
        "level",
        "target",
        "event",
        "message",
        "trace_id",
        "turn_id",
        "tool_name",
        "agent_id",
        "error_kind",
    ] {
        assert!(value.get(key).is_some(), "missing key: {key}");
    }
}

#[test]
fn structured_log_uses_string_timestamp() {
    let raw = format_event_json(
        "INFO",
        "blazar::app",
        "app_log",
        "logger initialized",
        None,
        None,
        None,
        None,
        None,
    );
    let value: Value = serde_json::from_str(&raw).expect("valid json");

    assert!(
        value["ts"].is_string(),
        "ts should be a string for stable downstream parsing"
    );
}
