//! Contracts for renderable chat view slots.

#[cfg(test)]
#[path = "../../../../tests/unit/chat/view/render_registry/tests.rs"]
mod tests;

use core::marker::PhantomData;
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

#[derive(Debug, Clone, Copy, Default)]
pub struct RenderCtx<'a> {
    pub _marker: PhantomData<&'a ()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    RegistryMissingSlot(RenderSlot),
    ComponentError(&'static str),
}

pub trait RenderUnit {
    fn render(&self, frame: &mut Frame, area: Rect, ctx: &RenderCtx<'_>)
    -> Result<(), RenderError>;
}
