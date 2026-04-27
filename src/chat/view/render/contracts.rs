//! Contracts for renderable chat view slots.

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

impl RenderSlot {
    pub const fn all_required() -> [RenderSlot; 7] {
        [
            RenderSlot::Timeline,
            RenderSlot::UsersTop,
            RenderSlot::UsersInput,
            RenderSlot::UsersModel,
            RenderSlot::UsersTopInputSeparator,
            RenderSlot::UsersInputModelSeparator,
            RenderSlot::PickerOverlay,
        ]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RenderCtx<'a> {
    _marker: PhantomData<&'a ()>,
}

impl<'a> RenderCtx<'a> {
    pub const fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl Default for RenderCtx<'_> {
    fn default() -> Self {
        Self::new()
    }
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

pub trait RenderRegistry {
    fn resolve(&self, slot: RenderSlot) -> Option<&dyn RenderUnit>;

    fn render_slot(
        &self,
        slot: RenderSlot,
        frame: &mut Frame,
        area: Rect,
        ctx: &RenderCtx<'_>,
    ) -> Result<(), RenderError> {
        let unit = self
            .resolve(slot)
            .ok_or(RenderError::RegistryMissingSlot(slot))?;
        unit.render(frame, area, ctx)
    }
}
