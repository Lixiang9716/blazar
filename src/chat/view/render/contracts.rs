//! Contracts for renderable chat view slots.

use crate::chat::app::ChatApp;
use crate::chat::theme::ChatTheme;
use crate::chat::users_state::UsersLayoutPolicy;
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

pub struct RenderCtx<'a> {
    app: &'a mut ChatApp,
    theme: ChatTheme,
    tick_ms: u64,
    users_policy: UsersLayoutPolicy,
}

impl<'a> RenderCtx<'a> {
    pub fn new(
        app: &'a mut ChatApp,
        theme: ChatTheme,
        tick_ms: u64,
        users_policy: UsersLayoutPolicy,
    ) -> Self {
        Self {
            app,
            theme,
            tick_ms,
            users_policy,
        }
    }

    pub fn app(&self) -> &ChatApp {
        self.app
    }

    pub fn app_mut(&mut self) -> &mut ChatApp {
        self.app
    }

    pub fn theme(&self) -> &ChatTheme {
        &self.theme
    }

    pub const fn tick_ms(&self) -> u64 {
        self.tick_ms
    }

    pub const fn users_policy(&self) -> UsersLayoutPolicy {
        self.users_policy
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    RegistryMissingSlot(RenderSlot),
    ComponentError(&'static str),
}

pub trait RenderUnit {
    fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        ctx: &mut RenderCtx<'_>,
    ) -> Result<(), RenderError>;
}

pub trait RenderRegistry {
    fn resolve(&self, slot: RenderSlot) -> Option<&dyn RenderUnit>;

    fn render_slot(
        &self,
        slot: RenderSlot,
        frame: &mut Frame,
        area: Rect,
        ctx: &mut RenderCtx<'_>,
    ) -> Result<(), RenderError> {
        let unit = self
            .resolve(slot)
            .ok_or(RenderError::RegistryMissingSlot(slot))?;
        unit.render(frame, area, ctx)
    }
}
