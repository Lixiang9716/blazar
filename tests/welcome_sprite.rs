use blazar::welcome::{mascot::render_mascot, sprite::SpriteAnimation, state::WelcomeState};

const SLIME_IDLE_PNG: &[u8] = include_bytes!("../assets/spirit/slime/slime_idle.png");

#[test]
fn welcome_sprite_slime_idle_sheet_decodes_into_four_terminal_frames() {
    let animation = SpriteAnimation::from_png_bytes(SLIME_IDLE_PNG, 4, 8)
        .expect("slime idle sprite sheet should decode into frames");

    assert_eq!(animation.len(), 4);
    for index in 0..4 {
        assert!(
            !animation.frame_by_index(index).to_ansi_string().is_empty(),
            "frame {index} should decode into terminal output"
        );
    }
}

#[test]
fn slime_idle_frame_exports_as_ansi_string() {
    let animation = SpriteAnimation::from_png_bytes(SLIME_IDLE_PNG, 4, 8)
        .expect("slime idle sprite sheet should decode into frames");

    let frame = animation.frame_by_index(0).to_ansi_string();

    assert!(frame.contains('\n'));
}

#[test]
fn render_mascot_starts_from_the_first_idle_frame() {
    let animation = SpriteAnimation::from_png_bytes(SLIME_IDLE_PNG, 4, 8)
        .expect("slime idle sprite sheet should decode into frames");

    assert_eq!(
        render_mascot(WelcomeState::new(), 0),
        animation.frame_by_index(0).to_ansi_string()
    );
}

#[test]
fn render_mascot_advances_across_idle_frames() {
    let animation = SpriteAnimation::from_png_bytes(SLIME_IDLE_PNG, 4, 8)
        .expect("slime idle sprite sheet should decode into frames");
    let idle_state = WelcomeState::new().tick(1_200, false);

    assert_eq!(
        render_mascot(idle_state, 1_200),
        animation.frame_by_index(0).to_ansi_string()
    );
    assert_eq!(
        render_mascot(idle_state, 1_325),
        animation.frame_by_index(1).to_ansi_string()
    );
}
