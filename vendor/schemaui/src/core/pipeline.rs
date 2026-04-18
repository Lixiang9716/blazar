use anyhow::Result;
use jsonschema::validator_for;
use ratatui::text::Line;
use serde_json::{Map, Value};

use crate::core::frontend::{Frontend, FrontendContext, HeaderAnimationSpec};
use crate::core::io::input::schema_with_defaults;
use crate::core::ui_ast::{UiAst, UiAstBundle, build_ui_ast_bundle};
use crate::schema::metadata::root_schema_header;

/// Core pipeline for preparing a `FrontendContext` from a base JSON Schema,
/// optional title, and optional default data.
///
/// This is the shared part of the flow:
///
/// ```text
/// io::input -> (schema, defaults) -> enriched schema -> ui_ast -> FrontendContext
/// ```
#[derive(Debug)]
enum UiAstSource {
    Runtime,
    Prepared(UiAstBundle),
}

#[derive(Debug)]
pub struct SchemaPipeline {
    schema: Value,
    title: Option<String>,
    header_lines: Option<Vec<Line<'static>>>,
    header_animation: Option<HeaderAnimationSpec>,
    defaults: Option<Value>,
    ui_ast_source: UiAstSource,
}

impl SchemaPipeline {
    pub fn new(schema: Value) -> Self {
        Self {
            schema,
            title: None,
            header_lines: None,
            header_animation: None,
            defaults: None,
            ui_ast_source: UiAstSource::Runtime,
        }
    }

    pub fn with_title(mut self, title: Option<String>) -> Self {
        self.title = title;
        self
    }

    pub fn with_header_lines(mut self, header_lines: Option<Vec<Line<'static>>>) -> Self {
        self.header_lines = header_lines;
        self
    }

    pub fn with_header_animation(mut self, header_animation: Option<HeaderAnimationSpec>) -> Self {
        self.header_animation = header_animation;
        self
    }

    pub fn with_defaults(mut self, defaults: Option<Value>) -> Self {
        self.defaults = defaults;
        self
    }

    /// Provide a prepared UiAst to be used instead of building one at runtime.
    /// If `ast` is None, the pipeline falls back to runtime UiAst building.
    pub fn with_prepared_ui_ast(mut self, ast: Option<UiAst>) -> Self {
        if let Some(ast) = ast {
            self.ui_ast_source = UiAstSource::Prepared(UiAstBundle::from_ui_ast(ast));
        }
        self
    }

    /// Provide a prepared bundle of shared UI artifacts.
    ///
    /// This lets the runtime reuse both `UiAst` and `UiLayout`, instead of
    /// rebuilding layout-oriented structures from the schema again.
    pub fn with_prepared_ui_bundle(mut self, bundle: Option<UiAstBundle>) -> Self {
        if let Some(bundle) = bundle {
            self.ui_ast_source = UiAstSource::Prepared(bundle);
        }
        self
    }

    fn build_frontend_context(self) -> Result<FrontendContext> {
        let SchemaPipeline {
            schema,
            title,
            header_lines,
            header_animation,
            defaults,
            ui_ast_source,
        } = self;

        let data = defaults.unwrap_or_else(|| Value::Object(Map::new()));
        let enriched = schema_with_defaults(&schema, &data);
        let (schema_title, schema_description) = root_schema_header(&enriched);

        let validator = validator_for(&enriched)?;
        let bundle = match ui_ast_source {
            UiAstSource::Runtime => build_ui_ast_bundle(&enriched)?,
            UiAstSource::Prepared(bundle) => bundle,
        };
        let (ui_ast, layout) = bundle.into_parts();

        Ok(FrontendContext {
            title: title.or(schema_title),
            description: schema_description,
            header_lines,
            header_animation,
            ui_ast,
            layout,
            initial_data: data,
            schema: enriched,
            validator,
        })
    }

    pub fn run_with_frontend<F>(self, frontend: F) -> Result<Value>
    where
        F: Frontend,
    {
        let ctx = self.build_frontend_context()?;
        frontend.run(ctx)
    }
}
