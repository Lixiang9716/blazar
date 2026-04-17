use blazar::welcome::sprite::SpriteAnimation;

const SLIME_IDLE_PNG: &[u8] = include_bytes!("../assets/spirit/slime/slime_idle.png");

#[test]
fn slime_idle_sheet_decodes_into_four_terminal_frames() {
    let animation = SpriteAnimation::from_png_bytes(SLIME_IDLE_PNG, 4, 8)
        .expect("slime idle sprite sheet should decode into frames");

    assert_eq!(animation.len(), 4);
    assert!(!animation.frame_by_index(0).to_ansi_string().is_empty());
}
