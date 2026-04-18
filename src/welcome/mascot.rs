use std::sync::OnceLock;

use ratatui::text::Line;

use crate::welcome::sprite::SpriteAnimation;
use crate::welcome::state::WelcomeState;

const SLIME_IDLE_PNG: &[u8] = include_bytes!("../../assets/spirit/slime/slime_idle.png");
const SLIME_IDLE_FRAMES: u32 = 4;
const SLIME_IDLE_FPS: u16 = 8;
const SLIME_IDLE_FRAME_MS: u64 = 1_000 / SLIME_IDLE_FPS as u64;

pub fn render_mascot(state: WelcomeState, now_ms: u64) -> String {
    let animation = slime_idle_animation();
    let frame_index = state.animation_frame_index(now_ms, animation.len(), SLIME_IDLE_FRAME_MS);

    animation.frame_by_index(frame_index).to_ansi_string()
}

pub fn schema_ui_header_lines() -> Vec<Line<'static>> {
    slime_idle_animation().frame_by_index(0).to_ratatui_lines()
}

fn slime_idle_animation() -> &'static SpriteAnimation {
    static ANIMATION: OnceLock<SpriteAnimation> = OnceLock::new();

    ANIMATION.get_or_init(|| {
        SpriteAnimation::from_png_bytes(SLIME_IDLE_PNG, SLIME_IDLE_FRAMES, SLIME_IDLE_FPS)
            .expect("slime idle sprite should decode")
    })
}
