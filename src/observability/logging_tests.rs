use crate::observability::logging::{
    emit_structured_event, format_event_json, with_captured_structured_events_for_test,
};
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

#[test]
fn structured_capture_isolation_is_stable_for_parallel_tests() {
    let thread_a = std::thread::spawn(|| {
        with_captured_structured_events_for_test(|| {
            emit_structured_event(
                log::Level::Info,
                "blazar::test",
                "capture_probe",
                "probe-a",
                None,
                Some("turn-a"),
                None,
                None,
                Some("ProviderTransient"),
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        })
        .1
    });

    let thread_b = std::thread::spawn(|| {
        with_captured_structured_events_for_test(|| {
            emit_structured_event(
                log::Level::Info,
                "blazar::test",
                "capture_probe",
                "probe-b",
                None,
                Some("turn-b"),
                None,
                None,
                Some("ProviderTransient"),
            );
        })
        .1
    });

    let events_a = thread_a.join().expect("thread A should capture events");
    let events_b = thread_b.join().expect("thread B should capture events");

    let turn_ids_a: Vec<String> = events_a
        .into_iter()
        .filter_map(|raw| serde_json::from_str::<Value>(&raw).ok())
        .filter_map(|event| {
            event
                .get("turn_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();
    let turn_ids_b: Vec<String> = events_b
        .into_iter()
        .filter_map(|raw| serde_json::from_str::<Value>(&raw).ok())
        .filter_map(|event| {
            event
                .get("turn_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();

    assert_eq!(turn_ids_a, vec!["turn-a".to_string()]);
    assert_eq!(turn_ids_b, vec!["turn-b".to_string()]);
}

#[test]
fn structured_capture_is_opt_in_for_current_thread_only() {
    let (_, captured) = with_captured_structured_events_for_test(|| {
        emit_structured_event(
            log::Level::Info,
            "blazar::test",
            "capture_probe",
            "captured",
            None,
            Some("turn-captured"),
            None,
            None,
            Some("ProviderTransient"),
        );
        std::thread::spawn(|| {
            emit_structured_event(
                log::Level::Info,
                "blazar::test",
                "capture_probe",
                "uncaptured",
                None,
                Some("turn-uncaptured"),
                None,
                None,
                Some("ProviderTransient"),
            );
        })
        .join()
        .expect("uncaptured emitter thread should complete");
    });

    let turn_ids: Vec<String> = captured
        .into_iter()
        .filter_map(|raw| serde_json::from_str::<Value>(&raw).ok())
        .filter_map(|event| {
            event
                .get("turn_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();

    assert_eq!(turn_ids, vec!["turn-captured".to_string()]);
}
