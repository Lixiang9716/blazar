//! Animation effects powered by tachyonfx.
//!
//! Wraps `tachyonfx::EffectManager` to provide Blazar-specific trigger
//! methods for picker slides, message fades, status pulses, banner
//! fade-in, and theme transitions.

use ratatui_core::{buffer::Buffer, layout::Rect, style::Color};
use tachyonfx::{Duration, EffectManager, Interpolation, Motion, fx, fx::RepeatMode};

/// Unique-effect keys so that overlapping triggers cancel the previous
/// animation of the same kind.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum EffectKind {
    #[default]
    PickerSlide,
    NewMessage,
    StatusPulse,
    BannerFade,
    ThemeTransition,
}

/// Thin wrapper around `EffectManager` with Blazar-specific helpers.
#[derive(Default)]
pub struct BlazarEffects {
    manager: EffectManager<EffectKind>,
}

impl BlazarEffects {
    /// Returns `true` when at least one animation is running.
    /// The event loop uses this to raise the frame rate during animations.
    pub fn is_running(&self) -> bool {
        self.manager.is_running()
    }

    /// Applies all active effects to the frame buffer.
    pub fn process(&mut self, elapsed: std::time::Duration, buf: &mut Buffer, area: Rect) {
        if !self.is_running() {
            return;
        }
        let dur = Duration::from_millis(elapsed.as_millis() as u32);
        self.manager.process_effects(dur, buf, area);
    }

    // -- trigger helpers ------------------------------------------------

    /// Picker opening: slide up + fade from dark.
    pub fn trigger_picker_open(&mut self, bg: Color) {
        let effect = fx::parallel(&[
            fx::slide_in(Motion::DownToUp, 8, 0, bg, (400, Interpolation::QuadOut)),
            fx::fade_from(bg, bg, (300, Interpolation::QuadOut)),
        ]);
        self.manager
            .add_unique_effect(EffectKind::PickerSlide, effect);
    }

    /// Picker closing: slide down + fade to dark.
    pub fn trigger_picker_close(&mut self, bg: Color) {
        let effect = fx::parallel(&[
            fx::slide_out(Motion::DownToUp, 8, 0, bg, (300, Interpolation::QuadIn)),
            fx::fade_to(bg, bg, (250, Interpolation::QuadIn)),
        ]);
        self.manager
            .add_unique_effect(EffectKind::PickerSlide, effect);
    }

    /// New timeline message: foreground fades in from black + coalesce.
    pub fn trigger_new_message(&mut self) {
        let black = Color::Black;
        let effect = fx::parallel(&[
            fx::fade_from_fg(black, (400, Interpolation::QuadOut)),
            fx::coalesce((350, Interpolation::QuadOut)),
        ]);
        self.manager
            .add_unique_effect(EffectKind::NewMessage, effect);
    }

    /// Status bar pulse: brief accent flash (ping-pong x2).
    pub fn trigger_status_pulse(&mut self, accent: Color) {
        let pulse = fx::fade_to_fg(accent, (300, Interpolation::SineInOut));
        let effect = fx::repeat(fx::ping_pong(pulse), RepeatMode::Times(2));
        self.manager
            .add_unique_effect(EffectKind::StatusPulse, effect);
    }

    /// Welcome banner fades in on startup.
    pub fn trigger_banner_fade(&mut self, bg: Color) {
        let effect = fx::sequence(&[
            fx::fade_from(bg, bg, (500, Interpolation::QuadOut)),
            fx::coalesce((400, Interpolation::QuadOut)),
        ]);
        self.manager
            .add_unique_effect(EffectKind::BannerFade, effect);
    }

    /// Full-screen transition when switching themes.
    pub fn trigger_theme_transition(&mut self, old_bg: Color, new_bg: Color) {
        let effect = fx::sequence(&[
            fx::fade_to(old_bg, old_bg, (200, Interpolation::QuadIn)),
            fx::fade_from(new_bg, new_bg, (300, Interpolation::QuadOut)),
        ]);
        self.manager
            .add_unique_effect(EffectKind::ThemeTransition, effect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_default_not_running() {
        let effects = BlazarEffects::default();
        assert!(!effects.is_running());
    }

    #[test]
    fn trigger_starts_running() {
        let mut effects = BlazarEffects::default();
        effects.trigger_new_message();
        assert!(effects.is_running());
    }

    #[test]
    fn process_completes_effect() {
        let mut effects = BlazarEffects::default();
        effects.trigger_new_message();
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 10));
        let area = Rect::new(0, 0, 40, 10);
        // Process for longer than any effect duration to ensure completion.
        effects.process(std::time::Duration::from_secs(2), &mut buf, area);
        assert!(!effects.is_running());
    }

    #[test]
    fn unique_effect_replaces_previous() {
        let mut effects = BlazarEffects::default();
        effects.trigger_picker_open(Color::Black);
        effects.trigger_picker_close(Color::Black);
        // Both add to the same key; after processing the old one is cancelled.
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 10));
        let area = Rect::new(0, 0, 40, 10);
        effects.process(std::time::Duration::from_millis(1), &mut buf, area);
        // Should still be running (the close effect hasn't finished).
        assert!(effects.is_running());
    }
}
