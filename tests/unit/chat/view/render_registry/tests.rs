use super::*;
use crate::chat::app::ChatApp;
use crate::chat::users_state::UsersLayoutPolicy;
use ratatui_core::{backend::TestBackend, layout::Rect, terminal::Terminal};

const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

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
            _ctx: &mut RenderCtx<'_>,
        ) -> Result<(), RenderError> {
            Ok(())
        }
    }

    let backend = TestBackend::new(1, 1);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let theme = app.theme().clone();
    let mut ctx = RenderCtx::new(&mut app, theme, 0, UsersLayoutPolicy::default());

    terminal
        .draw(|frame| {
            NoopRenderUnit
                .render(frame, Rect::new(0, 0, 1, 1), &mut ctx)
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
    let mut app = ChatApp::new_for_test(REPO_ROOT).expect("test app should initialize");
    let theme = app.theme().clone();
    let mut ctx = RenderCtx::new(&mut app, theme, 0, UsersLayoutPolicy::default());

    terminal
        .draw(|frame| {
            let err = EmptyRegistry
                .render_slot(RenderSlot::Timeline, frame, Rect::new(0, 0, 1, 1), &mut ctx)
                .expect_err("missing slots should return a registry error");

            assert!(matches!(
                err,
                RenderError::RegistryMissingSlot(RenderSlot::Timeline)
            ));
        })
        .expect("frame should render");
}
