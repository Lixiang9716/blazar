use crate::config;
use schemaui::prelude::*;
use serde_json::Value;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let schema = build_schema()?;
    let title = config::schema_title(&schema)?.to_owned();
    let value = SchemaUI::new(schema)
        .with_title(&title)
        .with_options(UiOptions::default())
        .run()?;

    println!("{}", serde_json::to_string_pretty(&value)?);

    Ok(())
}

fn build_schema() -> Result<Value, config::ConfigError> {
    config::load_app_schema()
}

#[cfg(test)]
mod tests {
    use super::build_schema;

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
