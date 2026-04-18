use anyhow::Result;
use ratatui::text::Line;
use serde_json::{Value, json};

use crate::{SchemaUI, core::frontend::{Frontend, FrontendContext}};

#[derive(Debug)]
struct CaptureFrontend;

impl Frontend for CaptureFrontend {
    fn run(self, ctx: FrontendContext) -> Result<Value> {
        Ok(json!({
            "header_lines": ctx.header_lines.as_ref().map(|lines| {
                lines.iter().map(ToString::to_string).collect::<Vec<_>>()
            }),
        }))
    }
}

#[test]
fn schema_ui_forwards_header_lines_into_frontend_context() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" }
        }
    });

    let result = SchemaUI::new(schema)
        .with_header_lines(vec![Line::from("SchemaUI mascot")])
        .run_with_frontend(CaptureFrontend)
        .expect("schema UI run succeeds");

    assert_eq!(result["header_lines"], json!(["SchemaUI mascot"]));
}
