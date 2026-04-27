use crate::chat::view::{
    render::contracts::{RenderCtx, RenderError, RenderUnit},
    timeline,
};
use ratatui_core::{layout::Rect, terminal::Frame};

pub(crate) struct TimelineRenderUnit;

impl RenderUnit for TimelineRenderUnit {
    fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        ctx: &mut RenderCtx<'_>,
    ) -> Result<(), RenderError> {
        timeline::render_timeline(frame, area, ctx.app(), ctx.theme());
        Ok(())
    }
}
