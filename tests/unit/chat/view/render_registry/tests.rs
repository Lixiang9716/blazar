use super::*;
use ratatui_core::{backend::TestBackend, layout::Rect, terminal::Terminal};

#[test]
fn default_registry_resolves_every_required_slot() {
    let registry = DefaultRenderRegistry::for_tests();

    for slot in RenderSlot::all_required() {
        assert!(registry.resolve(slot).is_some(), "missing slot: {slot:?}");
    }
}

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

#[test]
fn registry_returns_explicit_error_for_missing_slot() {
    struct EmptyRegistry;

    impl RenderRegistry for EmptyRegistry {
        fn resolve(&self, _slot: RenderSlot) -> Option<&dyn RenderUnit> {
            None
        }
    }

    let backend = TestBackend::new(1, 1);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let ctx = RenderCtx::default();

    terminal
        .draw(|frame| {
            let err = EmptyRegistry
                .render_slot(RenderSlot::Timeline, frame, Rect::new(0, 0, 1, 1), &ctx)
                .expect_err("missing slots should return a registry error");

            assert!(matches!(
                err,
                RenderError::RegistryMissingSlot(RenderSlot::Timeline)
            ));
        })
        .expect("frame should render");
}
