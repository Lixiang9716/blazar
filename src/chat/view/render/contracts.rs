use ratatui_core::{layout::Rect, terminal::Frame};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderSlot {
    Timeline,
    UsersTop,
    UsersInput,
    UsersModel,
    UsersTopInputSeparator,
    UsersInputModelSeparator,
    PickerOverlay,
}

#[derive(Debug, Default)]
pub struct RenderContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    RegistryMissingSlot(RenderSlot),
    ComponentError(&'static str),
}

pub trait RenderUnit {
    fn render(&self, frame: &mut Frame, area: Rect, ctx: &RenderContext)
    -> Result<(), RenderError>;
}
