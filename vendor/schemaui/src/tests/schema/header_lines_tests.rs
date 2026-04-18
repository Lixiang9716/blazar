use anyhow::Result;
use ratatui::text::Line;
use serde_json::{Value, json};
use std::time::Duration;

use crate::{SchemaUI, core::frontend::{Frontend, FrontendContext}};

#[derive(Debug)]
struct CaptureFrontend;

impl Frontend for CaptureFrontend {
    fn run(self, ctx: FrontendContext) -> Result<Value> {
        Ok(json!({
            "header_lines": ctx.header_lines.as_ref().map(|lines| {
                lines.iter().map(ToString::to_string).collect::<Vec<_>>()
            }),
            "header_animation": ctx.header_animation.as_ref().map(|animation| {
                json!({
                    "frame_interval_ms": animation.frame_interval.as_millis(),
                    "frame_count": animation.frames.len(),
                    "first_frame": animation.frames[0]
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>(),
                })
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

#[test]
fn schema_ui_forwards_header_animation_into_frontend_context() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" }
        }
    });

    let result = SchemaUI::new(schema)
        .with_header_animation(
            vec![
                vec![Line::from("frame 0")],
                vec![Line::from("frame 1")],
            ],
            Duration::from_millis(125),
        )
        .run_with_frontend(CaptureFrontend)
        .expect("schema UI run succeeds");

    assert_eq!(
        result["header_animation"],
        json!({
            "frame_interval_ms": 125,
            "frame_count": 2,
            "first_frame": ["frame 0"],
        })
    );
}
