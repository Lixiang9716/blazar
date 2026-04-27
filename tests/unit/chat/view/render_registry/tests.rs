use super::*;
use ratatui_core::{backend::TestBackend, layout::Rect, terminal::Terminal};

#[test]
fn render_slot_enum_covers_all_chat_surfaces() {
    let slots = [
        RenderSlot::Timeline,
        RenderSlot::UsersTop,
        RenderSlot::UsersInput,
        RenderSlot::UsersModel,
        RenderSlot::UsersTopInputSeparator,
        RenderSlot::UsersInputModelSeparator,
        RenderSlot::PickerOverlay,
    ];

    assert_eq!(slots.len(), 7);
}

#[test]
fn render_unit_uses_render_ctx_reference() {
    struct NoopRenderUnit;

    impl RenderUnit for NoopRenderUnit {
        fn render(
            &self,
            _frame: &mut Frame,
            _area: Rect,
            _ctx: &RenderCtx<'_>,
        ) -> Result<(), RenderError> {
            Ok(())
        }
    }

    let backend = TestBackend::new(1, 1);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let ctx = RenderCtx::default();

    terminal
        .draw(|frame| {
            NoopRenderUnit
                .render(frame, Rect::new(0, 0, 1, 1), &ctx)
                .expect("render unit should accept render ctx reference");
        })
        .expect("frame should render");
}
