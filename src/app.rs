use crate::config;
use schemaui::prelude::*;
use serde_json::Value;
use std::io;

fn build_schema() -> Result<Value, config::ConfigError> {
    config::load_app_schema()
}

type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

fn run_welcome_preview() -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();

    crate::welcome::startup::run_preview(&mut output)
}

fn run_schema_ui(schema: Value) -> AppResult<Value> {
    let title = config::schema_title(&schema)?.to_owned();
    let value = SchemaUI::new(schema)
        .with_title(&title)
        .with_options(UiOptions::default())
        .run()?;

    Ok(value)
}

fn run_flow<W, B, S>(run_welcome: W, build_schema: B, run_schema: S) -> AppResult<Value>
where
    W: FnOnce() -> std::io::Result<()>,
    B: FnOnce() -> Result<Value, config::ConfigError>,
    S: FnOnce(Value) -> AppResult<Value>,
{
    run_welcome()?;
    let schema = build_schema()?;
    run_schema(schema)
}

fn run_app<W, B, S, P>(
    run_welcome: W,
    build_schema: B,
    run_schema: S,
    print_json: P,
) -> AppResult<()>
where
    W: FnOnce() -> io::Result<()>,
    B: FnOnce() -> Result<Value, config::ConfigError>,
    S: FnOnce(Value) -> AppResult<Value>,
    P: FnOnce(String) -> AppResult<()>,
{
    let value = run_flow(run_welcome, build_schema, run_schema)?;
    let json = serde_json::to_string_pretty(&value)?;
    print_json(json)
}

pub fn run() -> AppResult<()> {
    run_app(run_welcome_preview, build_schema, run_schema_ui, |json| {
        println!("{json}");
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::{build_schema, run_app, run_flow};
    use serde_json::json;
    use std::cell::RefCell;
    use std::io;

    #[test]
    fn run_flow_runs_welcome_before_schema_ui() {
        let calls = RefCell::new(Vec::new());

        let value = run_flow(
            || {
                calls.borrow_mut().push("welcome");
                Ok(())
            },
            || {
                calls.borrow_mut().push("schema");
                Ok(json!({
                    "title": "Blazar",
                    "type": "object",
                    "properties": {}
                }))
            },
            |schema| {
                assert_eq!(schema["title"], "Blazar");
                calls.borrow_mut().push("ui");
                Ok(json!({"request": "ok"}))
            },
        )
        .expect("startup flow should succeed");

        assert_eq!(value["request"], "ok");
        assert_eq!(*calls.borrow(), vec!["welcome", "schema", "ui"]);
    }

    #[test]
    fn run_flow_bubbles_welcome_errors_without_loading_schema() {
        let calls = RefCell::new(Vec::new());

        let error = run_flow(
            || {
                calls.borrow_mut().push("welcome");
                Err(io::Error::new(io::ErrorKind::Other, "welcome failed"))
            },
            || {
                calls.borrow_mut().push("schema");
                build_schema()
            },
            |_schema| unreachable!("schema ui should not run after welcome failure"),
        )
        .expect_err("welcome failure should bubble up");

        assert!(error.to_string().contains("welcome failed"));
        assert_eq!(*calls.borrow(), vec!["welcome"]);
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
    fn run_app_prints_serialized_value_after_startup_flow() {
        let calls = RefCell::new(Vec::new());
        let printed = RefCell::new(String::new());

        run_app(
            || {
                calls.borrow_mut().push("welcome");
                Ok(())
            },
            || Ok(json!({"title": "Blazar", "type": "object", "properties": {}})),
            |_schema| {
                calls.borrow_mut().push("ui");
                Ok(json!({"delivery": {"format": "text"}}))
            },
            |json: String| {
                calls.borrow_mut().push("print");
                printed.borrow_mut().push_str(&json);
                Ok(())
            },
        )
        .expect("startup flow should succeed");

        assert!(printed.borrow().contains("\"delivery\""));
        assert_eq!(*calls.borrow(), vec!["welcome", "ui", "print"]);
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
