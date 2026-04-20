use crate::chat;
use crate::config;
use serde_json::Value;
use std::io::{self, BufRead, Write};

pub(crate) fn build_schema() -> Result<Value, config::ConfigError> {
    config::load_app_schema()
}

pub(crate) type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn runtime_name_for_test() -> &'static str {
    "spirit-chat-tui"
}

// Legacy prompt-flow implementation retained for migration reference
#[allow(dead_code)]
#[derive(Debug)]
enum PromptError {
    Io(io::Error),
    InvalidSchema {
        pointer: &'static str,
        expected: &'static str,
    },
    InvalidEnumDefault {
        pointer: &'static str,
        default: String,
    },
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::InvalidSchema { pointer, expected } => {
                write!(f, "invalid schema at {pointer}: expected {expected}")
            }
            Self::InvalidEnumDefault { pointer, default } => {
                write!(
                    f,
                    "invalid schema at {pointer}: default {default:?} is not in enum"
                )
            }
        }
    }
}

impl std::error::Error for PromptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidSchema { .. } | Self::InvalidEnumDefault { .. } => None,
        }
    }
}

impl From<io::Error> for PromptError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[allow(dead_code)]
fn run_prompt_flow<R: BufRead, W: Write>(
    schema: Value,
    input: &mut R,
    output: &mut W,
) -> AppResult<Value> {
    crate::welcome::startup::run_preview(output)?;
    collect_submission(&schema, input, output).map_err(Into::into)
}

#[allow(dead_code)]
fn collect_submission<R: BufRead, W: Write>(
    schema: &Value,
    input: &mut R,
    output: &mut W,
) -> Result<Value, PromptError> {
    writeln!(output)?;
    writeln!(output, "{}\n", string_at(schema, "/title")?)?;
    writeln!(output, "Press Enter to accept defaults.\n")?;

    let task_title = string_at(schema, "/properties/task/title")?;
    let workspace_title = string_at(schema, "/properties/workspace/title")?;
    let delivery_title = string_at(schema, "/properties/delivery/title")?;

    write_section_header(output, &task_title)?;
    let request = prompt_string(
        input,
        output,
        &string_at(schema, "/properties/task/properties/request/title")?,
        &string_at(schema, "/properties/task/properties/request/default")?,
    )?;
    let goal = prompt_string(
        input,
        output,
        &string_at(schema, "/properties/task/properties/goal/title")?,
        &string_at(schema, "/properties/task/properties/goal/default")?,
    )?;
    let priority = prompt_enum(
        input,
        output,
        &string_at(schema, "/properties/task/properties/priority/title")?,
        &string_at(schema, "/properties/task/properties/priority/default")?,
        &string_list_at(schema, "/properties/task/properties/priority/enum")?,
        "/properties/task/properties/priority/default",
    )?;

    write_section_header(output, &workspace_title)?;
    let repo_path = prompt_string(
        input,
        output,
        &string_at(schema, "/properties/workspace/properties/repoPath/title")?,
        &string_at(schema, "/properties/workspace/properties/repoPath/default")?,
    )?;
    let platform = prompt_string(
        input,
        output,
        &string_at(schema, "/properties/workspace/properties/platform/title")?,
        &string_at(schema, "/properties/workspace/properties/platform/default")?,
    )?;
    let interactive = prompt_bool(
        input,
        output,
        &string_at(schema, "/properties/workspace/properties/interactive/title")?,
        bool_at(
            schema,
            "/properties/workspace/properties/interactive/default",
        )?,
    )?;

    write_section_header(output, &delivery_title)?;
    let response_style = prompt_enum(
        input,
        output,
        &string_at(
            schema,
            "/properties/delivery/properties/responseStyle/title",
        )?,
        &string_at(
            schema,
            "/properties/delivery/properties/responseStyle/default",
        )?,
        &string_list_at(schema, "/properties/delivery/properties/responseStyle/enum")?,
        "/properties/delivery/properties/responseStyle/default",
    )?;
    let run_validation = prompt_bool(
        input,
        output,
        &string_at(
            schema,
            "/properties/delivery/properties/runValidation/title",
        )?,
        bool_at(
            schema,
            "/properties/delivery/properties/runValidation/default",
        )?,
    )?;
    let notes = prompt_string(
        input,
        output,
        &string_at(schema, "/properties/delivery/properties/notes/title")?,
        &string_at(schema, "/properties/delivery/properties/notes/default")?,
    )?;

    Ok(serde_json::json!({
        "task": {
            "request": request,
            "goal": goal,
            "priority": priority,
        },
        "workspace": {
            "repoPath": repo_path,
            "platform": platform,
            "interactive": interactive,
        },
        "delivery": {
            "responseStyle": response_style,
            "runValidation": run_validation,
            "notes": notes,
        }
    }))
}

#[allow(dead_code)]
fn write_section_header<W: Write>(output: &mut W, title: &str) -> io::Result<()> {
    writeln!(output, "{title}")?;
    writeln!(output, "{}", "-".repeat(title.chars().count()))
}

#[allow(dead_code)]
fn prompt_string<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    label: &str,
    default: &str,
) -> Result<String, PromptError> {
    let prompt = format!("{label} [{default}]: ");
    let response = read_prompt(input, output, &prompt)?;
    Ok(if response.is_empty() {
        default.to_owned()
    } else {
        response
    })
}

#[allow(dead_code)]
fn prompt_enum<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    label: &str,
    default: &str,
    choices: &[String],
    default_pointer: &'static str,
) -> Result<String, PromptError> {
    if !choices.iter().any(|choice| choice == default) {
        return Err(PromptError::InvalidEnumDefault {
            pointer: default_pointer,
            default: default.to_owned(),
        });
    }

    let prompt = format!("{label} ({}) [{default}]: ", choices.join("/"));

    loop {
        let response = read_prompt(input, output, &prompt)?;
        if response.is_empty() {
            return Ok(default.to_owned());
        }
        if choices.iter().any(|choice| choice == &response) {
            return Ok(response);
        }

        writeln!(output, "Please choose one of: {}", choices.join(", "))?;
    }
}

#[allow(dead_code)]
fn prompt_bool<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    label: &str,
    default: bool,
) -> Result<bool, PromptError> {
    let hint = if default { "Y/n" } else { "y/N" };
    let prompt = format!("{label} [{hint}]: ");

    loop {
        let response = read_prompt(input, output, &prompt)?;
        if response.is_empty() {
            return Ok(default);
        }

        match response.to_ascii_lowercase().as_str() {
            "y" | "yes" | "true" | "1" => return Ok(true),
            "n" | "no" | "false" | "0" => return Ok(false),
            _ => writeln!(output, "Please answer yes or no.")?,
        }
    }
}

#[allow(dead_code)]
fn read_prompt<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    prompt: &str,
) -> Result<String, PromptError> {
    write!(output, "{prompt}")?;
    output.flush()?;

    let mut line = String::new();
    let bytes_read = input.read_line(&mut line)?;
    if bytes_read == 0 {
        return Ok(String::new());
    }

    Ok(line.trim().to_owned())
}

#[allow(dead_code)]
fn string_at(schema: &Value, pointer: &'static str) -> Result<String, PromptError> {
    schema
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(PromptError::InvalidSchema {
            pointer,
            expected: "string",
        })
}

#[allow(dead_code)]
fn bool_at(schema: &Value, pointer: &'static str) -> Result<bool, PromptError> {
    schema
        .pointer(pointer)
        .and_then(Value::as_bool)
        .ok_or(PromptError::InvalidSchema {
            pointer,
            expected: "boolean",
        })
}

#[allow(dead_code)]
fn string_list_at(schema: &Value, pointer: &'static str) -> Result<Vec<String>, PromptError> {
    schema
        .pointer(pointer)
        .and_then(Value::as_array)
        .ok_or(PromptError::InvalidSchema {
            pointer,
            expected: "array of strings",
        })?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or(PromptError::InvalidSchema {
                    pointer,
                    expected: "array of strings",
                })
        })
        .collect()
}

#[allow(dead_code)]
fn run_app_with_io<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> AppResult<()> {
    let schema = build_schema()?;
    let value = run_prompt_flow(schema, input, output)?;
    writeln!(output)?;
    serde_json::to_writer_pretty(&mut *output, &value)?;
    writeln!(output)?;
    Ok(())
}

pub fn run() -> AppResult<()> {
    init_logger();
    log::info!("Blazar starting");
    let schema = build_schema()?;
    let mascot = config::load_mascot_config()?;
    chat::event_loop::run_terminal_chat(schema, mascot)
}

/// Initialize file-based logger.  Logs go to `logs/blazar.log` in the repo
/// root.  The TUI owns stdout/stderr so all logging must go to a file.
fn init_logger() {
    use flexi_logger::{FileSpec, Logger, WriteMode};

    let log_dir = std::env::current_dir().unwrap_or_default().join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    let level = std::env::var("BLAZAR_LOG").unwrap_or_else(|_| "debug".to_owned());

    if let Err(e) = Logger::try_with_str(&level).and_then(|logger| {
        logger
            .log_to_file(
                FileSpec::default()
                    .directory(log_dir)
                    .basename("blazar")
                    .suppress_timestamp(),
            )
            .write_mode(WriteMode::BufferAndFlush)
            .format(flexi_logger::detailed_format)
            .start()
    }) {
        eprintln!("Failed to init logger: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::{build_schema, collect_submission, run_app_with_io, run_prompt_flow};
    use serde_json::json;

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

    fn schema_property_names() -> Vec<String> {
        let schema = build_schema().expect("schema should load from config/app.json");
        let object = schema["properties"]
            .as_object()
            .expect("top-level properties should be an object");
        let mut keys: Vec<String> = object.keys().cloned().collect();
        keys.sort_unstable();
        keys
    }
}
