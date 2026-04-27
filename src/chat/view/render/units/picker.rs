use crate::chat::view::{
    self as view,
    render::contracts::{RenderCtx, RenderError, RenderUnit},
};
use ratatui_core::{layout::Rect, terminal::Frame};

pub(crate) struct PickerRenderUnit;

impl RenderUnit for PickerRenderUnit {
    fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        ctx: &mut RenderCtx<'_>,
    ) -> Result<(), RenderError> {
        view::render_picker_overlay_slot(frame, area, ctx);
        Ok(())
    }
}
