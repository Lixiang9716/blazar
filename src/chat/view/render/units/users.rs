use crate::chat::view::{
    render::contracts::{RenderCtx, RenderError, RenderUnit},
    users,
};
use ratatui_core::{layout::Rect, terminal::Frame};

pub(crate) struct UsersTopRenderUnit;
pub(crate) struct UsersInputRenderUnit;
pub(crate) struct UsersModelRenderUnit;
pub(crate) struct UsersTopInputSeparatorRenderUnit;
pub(crate) struct UsersInputModelSeparatorRenderUnit;

impl RenderUnit for UsersTopRenderUnit {
    fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        ctx: &mut RenderCtx<'_>,
    ) -> Result<(), RenderError> {
        let theme = ctx.theme().clone();
        let policy = ctx.users_policy();
        users::top_panel::render_top_panel(frame, area, ctx.app_mut(), &theme, policy);
        Ok(())
    }
}

impl RenderUnit for UsersInputRenderUnit {
    fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        ctx: &mut RenderCtx<'_>,
    ) -> Result<(), RenderError> {
        users::input_panel::render_input_panel(frame, area, ctx.app(), ctx.theme());
        Ok(())
    }
}

impl RenderUnit for UsersModelRenderUnit {
    fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        ctx: &mut RenderCtx<'_>,
    ) -> Result<(), RenderError> {
        users::model_panel::render_model_panel(frame, area, ctx.app(), ctx.theme());
        Ok(())
    }
}

impl RenderUnit for UsersTopInputSeparatorRenderUnit {
    fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        ctx: &mut RenderCtx<'_>,
    ) -> Result<(), RenderError> {
        users::panels::render_separator(frame, area, ctx.theme());
        Ok(())
    }
}

impl RenderUnit for UsersInputModelSeparatorRenderUnit {
    fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        ctx: &mut RenderCtx<'_>,
    ) -> Result<(), RenderError> {
        users::panels::render_separator(frame, area, ctx.theme());
        Ok(())
    }
}
