use super::{
    PromptError, bool_at, build_schema, collect_submission, init_logger, prompt_bool, prompt_enum,
    prompt_string, read_prompt, run_app_with_io, run_prompt_flow, string_at, string_list_at,
    write_section_header,
};
use serde_json::json;
use std::error::Error;
use std::sync::{Mutex, OnceLock};

static LOGGER_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[test]
fn collect_submission_uses_defaults_for_blank_answers() {
    let schema = build_schema().expect("schema should load from config/app.json");
    let mut input = std::io::Cursor::new("\n\n\n\n\n\n\n\n\n");
    let mut output = Vec::new();

    let submission =
        collect_submission(&schema, &mut input, &mut output).expect("defaults should submit");

    assert_eq!(
        submission,
        json!({
            "task": {
                "request": "Work on this repository with clear, safe steps",
                "goal": "Finish the requested coding task with verified changes",
                "priority": "normal"
            },
            "workspace": {
                "repoPath": "/home/lx/blazar",
                "platform": "Linux",
                "interactive": true
            },
            "delivery": {
                "responseStyle": "balanced",
                "runValidation": true,
                "notes": "Prefer useful, minimal changes"
            }
        })
    );
}

#[test]
fn collect_submission_reprompts_after_invalid_answers() {
    let schema = build_schema().expect("schema should load from config/app.json");
    let mut input = std::io::Cursor::new(
        "Ship mascot locally\nKeep spirit first\nextreme\nhigh\n/tmp/demo\nmacOS\nmaybe\nno\ndetailed\n0\nCustom notes\n",
    );
    let mut output = Vec::new();

    let submission =
        collect_submission(&schema, &mut input, &mut output).expect("answers should submit");
    let transcript = String::from_utf8(output).expect("prompt output should be utf-8");

    assert!(transcript.contains("Please choose one of: low, normal, high, urgent"));
    assert!(transcript.contains("Please answer yes or no."));
    assert_eq!(submission["task"]["priority"], "high");
    assert_eq!(submission["workspace"]["interactive"], false);
    assert_eq!(submission["delivery"]["responseStyle"], "detailed");
    assert_eq!(submission["delivery"]["runValidation"], false);
}

#[test]
fn run_prompt_flow_renders_welcome_before_questions() {
    let schema = build_schema().expect("schema should load from config/app.json");
    let mut input = std::io::Cursor::new("\n\n\n\n\n\n\n\n\n");
    let mut output = Vec::new();

    let value =
        run_prompt_flow(schema, &mut input, &mut output).expect("prompt flow should succeed");
    let transcript = String::from_utf8(output).expect("prompt output should be utf-8");

    assert!(transcript.contains("A rainbow helper just spotted you"));
    assert!(transcript.contains("Waiting with a sprinkle of stardust"));
    assert!(transcript.contains("Blazar Mission Console"));
    assert_eq!(value["workspace"]["repoPath"], "/home/lx/blazar");
}

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
fn run_app_prints_serialized_value_after_prompt_flow() {
    let mut input = std::io::Cursor::new("\n\n\n\n\n\n\n\n\n");
    let mut output = Vec::new();

    run_app_with_io(&mut input, &mut output).expect("app flow should succeed");

    let transcript = String::from_utf8(output).expect("app output should be utf-8");
    assert!(transcript.contains("\"delivery\""));
    assert!(transcript.contains("\"responseStyle\": \"balanced\""));
}

#[test]
fn prompt_string_returns_custom_value_when_provided() {
    let mut input = std::io::Cursor::new("custom\n");
    let mut output = Vec::new();

    let value = prompt_string(&mut input, &mut output, "Task", "default").expect("prompt succeeds");
    assert_eq!(value, "custom");
}

#[test]
fn prompt_enum_validates_default_and_reprompts_invalid_answer() {
    let mut input = std::io::Cursor::new("bad\nhigh\n");
    let mut output = Vec::new();
    let choices = vec!["low".to_string(), "high".to_string()];

    let value = prompt_enum(
        &mut input,
        &mut output,
        "Priority",
        "low",
        &choices,
        "/pointer/default",
    )
    .expect("enum prompt succeeds");
    let transcript = String::from_utf8(output).expect("utf-8");

    assert_eq!(value, "high");
    assert!(transcript.contains("Please choose one of: low, high"));
}

#[test]
fn prompt_enum_rejects_default_not_in_choices() {
    let mut input = std::io::Cursor::new("");
    let mut output = Vec::new();
    let choices = vec!["low".to_string(), "high".to_string()];

    let err = prompt_enum(
        &mut input,
        &mut output,
        "Priority",
        "normal",
        &choices,
        "/invalid/default",
    )
    .expect_err("default must be validated");
    assert!(matches!(
        err,
        PromptError::InvalidEnumDefault {
            pointer: "/invalid/default",
            ..
        }
    ));
}

#[test]
fn prompt_bool_handles_yes_no_and_reprompts_unknown_values() {
    let mut input = std::io::Cursor::new("maybe\ny\n");
    let mut output = Vec::new();
    let yes = prompt_bool(&mut input, &mut output, "Run validation", false)
        .expect("bool prompt succeeds");
    let transcript = String::from_utf8(output).expect("utf-8");
    assert!(yes);
    assert!(transcript.contains("Please answer yes or no."));

    let mut input = std::io::Cursor::new("0\n");
    let mut output = Vec::new();
    let no =
        prompt_bool(&mut input, &mut output, "Run validation", true).expect("bool prompt succeeds");
    assert!(!no);
}

#[test]
fn read_prompt_returns_empty_string_on_eof() {
    let mut input = std::io::Cursor::new("");
    let mut output = Vec::new();

    let value = read_prompt(&mut input, &mut output, "Prompt: ").expect("read_prompt");
    assert_eq!(value, "");
}

#[test]
fn schema_accessors_validate_types_and_pointers() {
    let schema = json!({
        "name": "Blazar",
        "enabled": true,
        "items": ["a", "b"]
    });
    assert_eq!(
        string_at(&schema, "/name").expect("string at"),
        "Blazar".to_string()
    );
    assert!(bool_at(&schema, "/enabled").expect("bool at"));
    assert_eq!(
        string_list_at(&schema, "/items").expect("list at"),
        vec!["a".to_string(), "b".to_string()]
    );

    assert!(matches!(
        string_at(&schema, "/enabled"),
        Err(PromptError::InvalidSchema {
            pointer: "/enabled",
            expected: "string"
        })
    ));
    assert!(matches!(
        bool_at(&schema, "/name"),
        Err(PromptError::InvalidSchema {
            pointer: "/name",
            expected: "boolean"
        })
    ));
    assert!(matches!(
        string_list_at(&json!({"items": [1]}), "/items"),
        Err(PromptError::InvalidSchema {
            pointer: "/items",
            expected: "array of strings"
        })
    ));
}

#[test]
fn write_section_header_writes_title_and_underline() {
    let mut output = Vec::new();
    write_section_header(&mut output, "Task").expect("header writes");
    let text = String::from_utf8(output).expect("utf-8");
    assert_eq!(text, "Task\n----\n");
}

#[test]
fn prompt_error_display_and_source_are_descriptive() {
    let io_error = std::io::Error::other("boom");
    let err = PromptError::Io(io_error);
    assert_eq!(err.to_string(), "boom");
    assert!(err.source().is_some());

    let invalid_schema = PromptError::InvalidSchema {
        pointer: "/x",
        expected: "string",
    };
    assert!(
        invalid_schema
            .to_string()
            .contains("invalid schema at /x: expected string")
    );
    assert!(invalid_schema.source().is_none());

    let invalid_default = PromptError::InvalidEnumDefault {
        pointer: "/y",
        default: "z".to_string(),
    };
    assert!(
        invalid_default
            .to_string()
            .contains("invalid schema at /y: default \"z\" is not in enum")
    );
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

    let log_dir = std::env::current_dir().expect("cwd").join("logs");
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
