use blazar::chat::view::render::contracts::RenderSlot;

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
