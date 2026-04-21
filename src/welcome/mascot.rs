use std::fs;
use std::io;
use std::sync::OnceLock;

use crate::config::{self, MascotConfig};
use crate::welcome::sprite::{SpriteAnimation, SpriteError, TerminalFrame};
use crate::welcome::state::WelcomeState;
use log::warn;
use ratatui_core::text::Line;

pub fn render_mascot(state: WelcomeState, now_ms: u64) -> String {
    render_mascot_with_assets(state, now_ms, mascot_assets().as_ref().ok())
}

pub fn render_mascot_plain(state: WelcomeState, now_ms: u64) -> String {
    render_mascot_plain_with_assets(state, now_ms, mascot_assets().as_ref().ok())
}

pub fn render_mascot_lines(state: WelcomeState, now_ms: u64) -> Vec<Line<'static>> {
    render_mascot_lines_with_assets(state, now_ms, mascot_assets().as_ref().ok())
}

const FALLBACK_MASCOT_TEXT: &str = "";

struct MascotAssets {
    config: MascotConfig,
    animation: SpriteAnimation,
}

#[derive(Debug)]
enum MascotLoadError {
    Config(config::ConfigError),
    AssetRead { path: String, source: io::Error },
    SpriteDecode(SpriteError),
}

impl std::fmt::Display for MascotLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(source) => write!(f, "{source}"),
            Self::AssetRead { path, source } => {
                write!(f, "failed to read mascot asset {path}: {source}")
            }
            Self::SpriteDecode(source) => write!(f, "{source}"),
        }
    }
}

fn mascot_assets() -> &'static Result<MascotAssets, MascotLoadError> {
    static ASSETS: OnceLock<Result<MascotAssets, MascotLoadError>> = OnceLock::new();

    ASSETS.get_or_init(|| {
        let loaded = load_mascot_assets();
        if let Err(error) = &loaded {
            warn!("welcome mascot disabled; using fallback: {error}");
        }
        loaded
    })
}

fn load_mascot_assets() -> Result<MascotAssets, MascotLoadError> {
    let config = config::load_mascot_config().map_err(MascotLoadError::Config)?;
    let png = fs::read(&config.asset_path).map_err(|source| MascotLoadError::AssetRead {
        path: config.asset_path.clone(),
        source,
    })?;
    let animation = SpriteAnimation::from_png_bytes(&png, config.frame_count, config.fps)
        .map_err(MascotLoadError::SpriteDecode)?;

    Ok(MascotAssets { config, animation })
}

fn current_frame(
    state: WelcomeState,
    now_ms: u64,
    assets: Option<&MascotAssets>,
) -> Option<&TerminalFrame> {
    assets.map(|assets| {
        let frame_index = state.animation_frame_index(
            now_ms,
            assets.animation.len(),
            assets.config.frame_interval_ms(),
        );
        assets.animation.frame_by_index(frame_index)
    })
}

fn render_mascot_with_assets(
    state: WelcomeState,
    now_ms: u64,
    assets: Option<&MascotAssets>,
) -> String {
    current_frame(state, now_ms, assets)
        .map(TerminalFrame::to_ansi_string)
        .unwrap_or_else(|| FALLBACK_MASCOT_TEXT.to_owned())
}

fn render_mascot_plain_with_assets(
    state: WelcomeState,
    now_ms: u64,
    assets: Option<&MascotAssets>,
) -> String {
    current_frame(state, now_ms, assets)
        .map(TerminalFrame::to_plain_string)
        .unwrap_or_else(|| FALLBACK_MASCOT_TEXT.to_owned())
}

fn render_mascot_lines_with_assets(
    state: WelcomeState,
    now_ms: u64,
    assets: Option<&MascotAssets>,
) -> Vec<Line<'static>> {
    current_frame(state, now_ms, assets)
        .map(TerminalFrame::to_styled_lines)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_mascot_falls_back_when_assets_fail_to_load() {
        let rendered = render_mascot_with_assets(WelcomeState::new(), 0, None);
        assert_eq!(rendered, FALLBACK_MASCOT_TEXT);
    }

    #[test]
    fn render_mascot_plain_falls_back_when_assets_fail_to_load() {
        let rendered = render_mascot_plain_with_assets(WelcomeState::new(), 0, None);
        assert_eq!(rendered, FALLBACK_MASCOT_TEXT);
    }

    #[test]
    fn render_mascot_lines_fall_back_when_assets_fail_to_load() {
        let rendered = std::panic::catch_unwind(|| {
            render_mascot_lines_with_assets(WelcomeState::new(), 0, None)
        })
        .expect("fallback rendering should not panic");
        assert!(rendered.is_empty());
    }
}
