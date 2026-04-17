use blazar::welcome::sprite::SpriteAnimation;
use ratatui::text::Line;

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
fn slime_idle_frame_exports_as_ratatui_lines() {
    let animation = SpriteAnimation::from_png_bytes(SLIME_IDLE_PNG, 4, 8)
        .expect("slime idle sprite sheet should decode into frames");

    let lines: Vec<Line<'static>> = animation.frame_by_index(0).to_ratatui_lines();

    assert!(lines.len() > 1);
}
