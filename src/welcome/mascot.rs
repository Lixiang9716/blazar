use std::fs;
use std::sync::OnceLock;

use crate::config::{self, MascotConfig};
use crate::welcome::sprite::SpriteAnimation;
use crate::welcome::state::WelcomeState;
use ratatui_core::text::Line;

pub fn render_mascot(state: WelcomeState, now_ms: u64) -> String {
    let config = slime_idle_config();
    let animation = slime_idle_animation();
    let frame_index =
        state.animation_frame_index(now_ms, animation.len(), config.frame_interval_ms());

    animation.frame_by_index(frame_index).to_ansi_string()
}

pub fn render_mascot_plain(state: WelcomeState, now_ms: u64) -> String {
    let config = slime_idle_config();
    let animation = slime_idle_animation();
    let frame_index =
        state.animation_frame_index(now_ms, animation.len(), config.frame_interval_ms());

    animation.frame_by_index(frame_index).to_plain_string()
}

pub fn render_mascot_lines(state: WelcomeState, now_ms: u64) -> Vec<Line<'static>> {
    let config = slime_idle_config();
    let animation = slime_idle_animation();
    let frame_index =
        state.animation_frame_index(now_ms, animation.len(), config.frame_interval_ms());

    animation.frame_by_index(frame_index).to_styled_lines()
}

fn slime_idle_config() -> &'static MascotConfig {
    static CONFIG: OnceLock<MascotConfig> = OnceLock::new();

    CONFIG.get_or_init(|| {
        config::load_mascot_config().expect("bundled mascot config should load from config/app.json")
    })
}

fn slime_idle_animation() -> &'static SpriteAnimation {
    static ANIMATION: OnceLock<SpriteAnimation> = OnceLock::new();

    ANIMATION.get_or_init(|| {
        let config = slime_idle_config();
        let png = fs::read(&config.asset_path).expect("slime idle sprite should be readable");

        SpriteAnimation::from_png_bytes(&png, config.frame_count, config.fps)
            .expect("slime idle sprite should decode")
    })
}
