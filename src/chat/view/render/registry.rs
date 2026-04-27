#![cfg_attr(not(test), allow(dead_code))]

use super::contracts::{RenderCtx, RenderError, RenderRegistry, RenderSlot, RenderUnit};
use ratatui_core::{layout::Rect, terminal::Frame};

#[cfg(test)]
#[path = "../../../../tests/unit/chat/view/render_registry/tests.rs"]
mod render_registry_tests;

pub(crate) struct DefaultRenderRegistry {
    timeline: TimelineRenderUnit,
    users_top: UsersTopRenderUnit,
    users_input: UsersInputRenderUnit,
    users_model: UsersModelRenderUnit,
    users_top_input_separator: UsersTopInputSeparatorRenderUnit,
    users_input_model_separator: UsersInputModelSeparatorRenderUnit,
    picker_overlay: PickerOverlayRenderUnit,
}

impl DefaultRenderRegistry {
    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        Self::default()
    }
}

impl Default for DefaultRenderRegistry {
    fn default() -> Self {
        Self {
            timeline: TimelineRenderUnit,
            users_top: UsersTopRenderUnit,
            users_input: UsersInputRenderUnit,
            users_model: UsersModelRenderUnit,
            users_top_input_separator: UsersTopInputSeparatorRenderUnit,
            users_input_model_separator: UsersInputModelSeparatorRenderUnit,
            picker_overlay: PickerOverlayRenderUnit,
        }
    }
}

impl RenderRegistry for DefaultRenderRegistry {
    fn resolve(&self, slot: RenderSlot) -> Option<&dyn RenderUnit> {
        match slot {
            RenderSlot::Timeline => Some(&self.timeline),
            RenderSlot::UsersTop => Some(&self.users_top),
            RenderSlot::UsersInput => Some(&self.users_input),
            RenderSlot::UsersModel => Some(&self.users_model),
            RenderSlot::UsersTopInputSeparator => Some(&self.users_top_input_separator),
            RenderSlot::UsersInputModelSeparator => Some(&self.users_input_model_separator),
            RenderSlot::PickerOverlay => Some(&self.picker_overlay),
        }
    }
}

struct TimelineRenderUnit;
struct UsersTopRenderUnit;
struct UsersInputRenderUnit;
struct UsersModelRenderUnit;
struct UsersTopInputSeparatorRenderUnit;
struct UsersInputModelSeparatorRenderUnit;
struct PickerOverlayRenderUnit;

macro_rules! impl_noop_render_unit {
    ($($unit:ty),+ $(,)?) => {
        $(
            impl RenderUnit for $unit {
                fn render(
                    &self,
                    _frame: &mut Frame,
                    _area: Rect,
                    _ctx: &RenderCtx<'_>,
                ) -> Result<(), RenderError> {
                    Ok(())
                }
            }
        )+
    };
}

impl_noop_render_unit!(
    TimelineRenderUnit,
    UsersTopRenderUnit,
    UsersInputRenderUnit,
    UsersModelRenderUnit,
    UsersTopInputSeparatorRenderUnit,
    UsersInputModelSeparatorRenderUnit,
    PickerOverlayRenderUnit,
);
