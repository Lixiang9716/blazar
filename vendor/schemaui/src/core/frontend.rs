use anyhow::Result;
use jsonschema::Validator;
use ratatui::text::Line;
use serde_json::Value;
use std::time::Duration;

use crate::core::ui_ast::{UiAst, UiLayout};

#[derive(Debug)]
pub struct HeaderAnimationSpec {
    pub frames: Vec<Vec<Line<'static>>>,
    pub frame_interval: Duration,
}

/// Shared context prepared by the core pipeline and consumed by frontends
/// (TUI, Web, or others).
#[derive(Debug)]
pub struct FrontendContext {
    pub title: Option<String>,
    pub description: Option<String>,
    pub header_lines: Option<Vec<Line<'static>>>,
    pub header_animation: Option<HeaderAnimationSpec>,
    pub ui_ast: UiAst,
    pub layout: UiLayout,
    pub initial_data: Value,
    pub schema: Value,
    pub validator: Validator,
}

/// Pluggable frontend interface. A frontend receives a `FrontendContext`,
/// renders an interactive UI, and returns the final edited value.
pub trait Frontend {
    fn run(self, ctx: FrontendContext) -> Result<Value>;
}
