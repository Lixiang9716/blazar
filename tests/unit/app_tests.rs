use super::{build_schema, init_logger};
use std::sync::{Mutex, OnceLock};

static LOGGER_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[test]
fn schema_keeps_request_field_required() {
    let schema = build_schema().expect("schema should load from config/app.json");
    let required = schema["properties"]["task"]["required"]
        .as_array()
        .expect("task.required should be an array");

    assert!(required.iter().any(|item| item == "request"));
}

#[test]
fn schema_exposes_three_top_level_sections() {
    let properties = schema_property_names();

    assert_eq!(properties, ["delivery", "task", "workspace"]);
}

#[test]
fn init_logger_writes_json_lines() {
    let _guard = LOGGER_ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("lock env");

    let old = std::env::var("BLAZAR_LOG").ok();
    unsafe { std::env::set_var("BLAZAR_LOG", "warn, blazar=debug") };

    init_logger();
    log::info!("logger_json_probe");
    log::logger().flush();

    let log_dir = std::env::current_dir()
        .expect("cwd")
        .join("target")
        .join("test-logs");
    let log_path = if log_dir.join("blazar_rCURRENT.log").exists() {
        log_dir.join("blazar_rCURRENT.log")
    } else {
        log_dir.join("blazar.log")
    };
    let text = std::fs::read_to_string(log_path).expect("log file");
    let matching_line = text
        .lines()
        .rev()
        .find(|line| line.contains("logger_json_probe"))
        .expect("probe line should exist");
    let value: serde_json::Value = serde_json::from_str(matching_line).expect("json line");
    assert_eq!(value["event"], "app_log");
    assert_eq!(value["message"], "logger_json_probe");
    assert!(value.get("level").is_some());

    unsafe {
        match old {
            Some(value) => std::env::set_var("BLAZAR_LOG", value),
            None => std::env::remove_var("BLAZAR_LOG"),
        }
    }
}

#[test]
fn init_logger_is_safe_to_call_multiple_times() {
    let _guard = LOGGER_ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("lock env");

    let old = std::env::var("BLAZAR_LOG").ok();
    unsafe { std::env::set_var("BLAZAR_LOG", "warn, blazar=debug") };
    init_logger();
    init_logger();

    unsafe {
        match old {
            Some(value) => std::env::set_var("BLAZAR_LOG", value),
            None => std::env::remove_var("BLAZAR_LOG"),
        }
    }
}

fn schema_property_names() -> Vec<String> {
    let schema = build_schema().expect("schema should load from config/app.json");
    let object = schema["properties"]
        .as_object()
        .expect("top-level properties should be an object");
    let mut keys: Vec<String> = object.keys().cloned().collect();
    keys.sort_unstable();
    keys
}
