use crate::config;
use schemaui::prelude::*;
use serde_json::Value;
use std::time::Duration;

fn build_schema() -> Result<Value, config::ConfigError> {
    config::load_app_schema()
}

type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

struct SchemaUiLaunch {
    schema: Value,
    title: String,
    header_animation_frames: Vec<Vec<ratatui::text::Line<'static>>>,
    header_frame_interval: Duration,
    ui_options: UiOptions,
}

fn prepare_schema_ui(schema: Value) -> Result<SchemaUiLaunch, config::ConfigError> {
    let title = config::schema_title(&schema)?.to_owned();
    let header_animation_frames = crate::welcome::mascot::schema_ui_header_animation_frames();
    let header_frame_interval = crate::welcome::mascot::schema_ui_header_animation_frame_interval();
    let ui_options = UiOptions::default().with_tick_rate(header_frame_interval);

    Ok(SchemaUiLaunch {
        schema,
        title,
        header_animation_frames,
        header_frame_interval,
        ui_options,
    })
}

fn run_schema_ui(schema: Value) -> AppResult<Value> {
    let launch = prepare_schema_ui(schema)?;
    let value = SchemaUI::new(launch.schema)
        .with_title(&launch.title)
        .with_header_animation(launch.header_animation_frames, launch.header_frame_interval)
        .with_options(launch.ui_options)
        .run()?;

    Ok(value)
}

fn run_flow<B, S>(build_schema: B, run_schema: S) -> AppResult<Value>
where
    B: FnOnce() -> Result<Value, config::ConfigError>,
    S: FnOnce(Value) -> AppResult<Value>,
{
    let schema = build_schema()?;
    run_schema(schema)
}

fn run_app<B, S, P>(build_schema: B, run_schema: S, print_json: P) -> AppResult<()>
where
    B: FnOnce() -> Result<Value, config::ConfigError>,
    S: FnOnce(Value) -> AppResult<Value>,
    P: FnOnce(String) -> AppResult<()>,
{
    let value = run_flow(build_schema, run_schema)?;
    let json = serde_json::to_string_pretty(&value)?;
    print_json(json)
}

pub fn run() -> AppResult<()> {
    run_app(build_schema, run_schema_ui, |json| {
        println!("{json}");
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::{build_schema, prepare_schema_ui, run_app, run_flow};
    use crate::config;
    use serde_json::json;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn prepare_schema_ui_sets_title_and_mascot_idle_animation() {
        let schema = json!({
            "title": "Blazar",
            "type": "object",
            "properties": {}
        });

        let launch = prepare_schema_ui(schema.clone()).expect("schema ui launch should build");

        assert_eq!(launch.schema, schema);
        assert_eq!(launch.title, "Blazar");
        assert_eq!(
            launch.header_animation_frames,
            crate::welcome::mascot::schema_ui_header_animation_frames()
        );
        assert_eq!(launch.header_frame_interval, Duration::from_millis(125));
        assert_eq!(launch.ui_options.tick_rate, launch.header_frame_interval);
    }

    #[test]
    fn run_flow_runs_schema_before_schema_ui() {
        let calls = RefCell::new(Vec::new());

        let value = run_flow(
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
        assert_eq!(*calls.borrow(), vec!["schema", "ui"]);
    }

    #[test]
    fn run_flow_bubbles_schema_errors_without_running_ui() {
        let calls = RefCell::new(Vec::new());

        let error = run_flow(
            || {
                calls.borrow_mut().push("schema");
                Err(config::ConfigError::InvalidSchema {
                    path: PathBuf::from(config::APP_SCHEMA_PATH),
                    message: "schema title must be a string",
                })
            },
            |_schema| unreachable!("schema ui should not run after schema load failure"),
        )
        .expect_err("schema failure should bubble up");

        assert!(error.to_string().contains("schema title must be a string"));
        assert_eq!(*calls.borrow(), vec!["schema"]);
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
                calls.borrow_mut().push("schema");
                Ok(json!({"title": "Blazar", "type": "object", "properties": {}}))
            },
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
        assert_eq!(*calls.borrow(), vec!["schema", "ui", "print"]);
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
