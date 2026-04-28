//! ```rust
//! use blazar::welcome::sprite::SpriteAnimation;
//!
//! let animation = SpriteAnimation::from_png_bytes(
//!     include_bytes!("../../assets/spirit/slime/slime_idle.png"),
//!     4,
//!     8,
//! )?;
//! let _frame = animation.frame_by_index(0).to_ansi_string();
//! # Ok::<(), blazar::welcome::sprite::SpriteError>(())
//! ```

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    time::{Duration, Instant},
};

use image::{Rgba, RgbaImage};
use ratatui_core::{
    style::{Color, Style},
    text::{Line, Span},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCell {
    glyph: char,
    fg: Option<Rgb>,
    bg: Option<Rgb>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalFrame {
    rows: Vec<Vec<TerminalCell>>,
}

impl TerminalFrame {
    pub fn to_ansi_string(&self) -> String {
        let mut out = String::new();

        for (row_index, row) in self.rows.iter().enumerate() {
            for cell in row {
                match (cell.fg, cell.bg) {
                    (None, None) => out.push(' '),
                    (fg, bg) => {
                        if let Some(Rgb(r, g, b)) = fg {
                            out.push_str(&format!("\u{1b}[38;2;{r};{g};{b}m"));
                        }
                        if let Some(Rgb(r, g, b)) = bg {
                            out.push_str(&format!("\u{1b}[48;2;{r};{g};{b}m"));
                        }
                        out.push(cell.glyph);
                        out.push_str("\u{1b}[0m");
                    }
                }
            }

            if row_index + 1 != self.rows.len() {
                out.push('\n');
            }
        }

        out
    }

    pub fn to_plain_string(&self) -> String {
        let mut out = String::new();

        for (row_index, row) in self.rows.iter().enumerate() {
            for cell in row {
                out.push(cell.glyph);
            }

            if row_index + 1 != self.rows.len() {
                out.push('\n');
            }
        }

        out
    }

    pub fn to_styled_lines(&self) -> Vec<Line<'static>> {
        self.rows
            .iter()
            .map(|row| {
                let spans = row
                    .iter()
                    .map(|cell| {
                        let mut style = Style::default();
                        if let Some(Rgb(r, g, b)) = cell.fg {
                            style = style.fg(Color::Rgb(r, g, b));
                        }
                        if let Some(Rgb(r, g, b)) = cell.bg {
                            style = style.bg(Color::Rgb(r, g, b));
                        }

                        Span::styled(cell.glyph.to_string(), style)
                    })
                    .collect::<Vec<_>>();

                Line::from(spans)
            })
            .collect()
    }
}

#[allow(dead_code)]
pub struct SpriteAnimation {
    frames: Vec<TerminalFrame>,
    current: usize,
    frame_time: Duration,
    last_tick: Instant,
}

impl SpriteAnimation {
    pub fn from_png_bytes(png: &[u8], frame_count: u32, fps: u16) -> Result<Self, SpriteError> {
        if frame_count == 0 {
            return Err(SpriteError::InvalidFrameCount);
        }
        if fps == 0 {
            return Err(SpriteError::InvalidFps);
        }

        let sheet = image::load_from_memory(png)?.to_rgba8();
        let width = sheet.width();
        let height = sheet.height();

        if width % frame_count != 0 {
            return Err(SpriteError::WidthNotDivisible { width, frame_count });
        }

        let frame_width = width / frame_count;
        let mut frames = Vec::with_capacity(frame_count as usize);

        for frame_index in 0..frame_count {
            let x_offset = frame_index * frame_width;
            frames.push(build_frame(&sheet, x_offset, frame_width, height));
        }

        Ok(Self {
            frames,
            current: 0,
            frame_time: Duration::from_nanos(1_000_000_000 / u64::from(fps)),
            last_tick: Instant::now(),
        })
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn frame_by_index(&self, index: usize) -> &TerminalFrame {
        &self.frames[index % self.frames.len()]
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick);
        let intervals = elapsed.as_nanos() / self.frame_time.as_nanos();

        if intervals > 0 {
            self.current = (self.current + intervals as usize) % self.frames.len();
            let remainder = elapsed.as_nanos() % self.frame_time.as_nanos();
            self.last_tick = now - Duration::from_nanos(remainder as u64);
        }
    }

    pub fn frame(&self) -> &TerminalFrame {
        &self.frames[self.current]
    }
}

#[derive(Debug)]
pub enum SpriteError {
    InvalidFrameCount,
    InvalidFps,
    WidthNotDivisible { width: u32, frame_count: u32 },
    Decode(image::ImageError),
}

impl Display for SpriteError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFrameCount => write!(f, "frame_count must be greater than 0"),
            Self::InvalidFps => write!(f, "fps must be greater than 0"),
            Self::WidthNotDivisible { width, frame_count } => write!(
                f,
                "sprite sheet width {width} is not divisible by frame count {frame_count}"
            ),
            Self::Decode(err) => write!(f, "failed to decode sprite sheet: {err}"),
        }
    }
}

impl Error for SpriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Decode(err) => Some(err),
            _ => None,
        }
    }
}

impl From<image::ImageError> for SpriteError {
    fn from(value: image::ImageError) -> Self {
        Self::Decode(value)
    }
}

fn build_frame(
    sheet: &RgbaImage,
    x_offset: u32,
    frame_width: u32,
    frame_height: u32,
) -> TerminalFrame {
    let mut rows = Vec::new();
    let mut y = 0;

    while y < frame_height {
        let mut row = Vec::with_capacity(frame_width as usize);

        for x in 0..frame_width {
            let top = *sheet.get_pixel(x_offset + x, y);
            let bottom = if y + 1 < frame_height {
                *sheet.get_pixel(x_offset + x, y + 1)
            } else {
                Rgba([0, 0, 0, 0])
            };

            row.push(pixel_pair_to_cell(top, bottom));
        }

        rows.push(row);
        y += 2;
    }

    TerminalFrame { rows }
}

fn pixel_pair_to_cell(top: Rgba<u8>, bottom: Rgba<u8>) -> TerminalCell {
    let top_visible = top[3] >= 16;
    let bottom_visible = bottom[3] >= 16;

    match (top_visible, bottom_visible) {
        (false, false) => TerminalCell {
            glyph: ' ',
            fg: None,
            bg: None,
        },
        (true, false) => TerminalCell {
            glyph: '▀',
            fg: Some(Rgb(top[0], top[1], top[2])),
            bg: None,
        },
        (false, true) => TerminalCell {
            glyph: '▄',
            fg: Some(Rgb(bottom[0], bottom[1], bottom[2])),
            bg: None,
        },
        (true, true) if top.0[..3] == bottom.0[..3] => TerminalCell {
            glyph: '█',
            fg: Some(Rgb(top[0], top[1], top[2])),
            bg: None,
        },
        (true, true) => TerminalCell {
            glyph: '▀',
            fg: Some(Rgb(top[0], top[1], top[2])),
            bg: Some(Rgb(bottom[0], bottom[1], bottom[2])),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn animation_with_frame_time() -> SpriteAnimation {
        SpriteAnimation::from_png_bytes(
            include_bytes!("../../assets/spirit/slime/slime_idle.png"),
            4,
            2,
        )
        .expect("slime idle sprite sheet should decode into frames")
    }

    #[test]
    fn transparent_pair_becomes_space() {
        let cell = pixel_pair_to_cell(Rgba([0, 0, 0, 0]), Rgba([0, 0, 0, 0]));

        assert_eq!(cell.glyph, ' ');
        assert_eq!(cell.fg, None);
        assert_eq!(cell.bg, None);
    }

    #[test]
    fn same_color_pair_becomes_full_block() {
        let cell = pixel_pair_to_cell(Rgba([1, 2, 3, 255]), Rgba([1, 2, 3, 255]));

        assert_eq!(cell.glyph, '█');
        assert_eq!(cell.fg, Some(Rgb(1, 2, 3)));
        assert_eq!(cell.bg, None);
    }

    #[test]
    fn tick_does_not_advance_before_one_interval() {
        let mut animation = animation_with_frame_time();
        let current = animation.current;
        animation.last_tick = Instant::now();

        animation.tick();

        assert_eq!(animation.current, current);
    }

    #[test]
    fn tick_advances_after_one_interval() {
        let mut animation = animation_with_frame_time();
        animation.current = 0;
        animation.last_tick = Instant::now() - animation.frame_time;

        animation.tick();

        assert_eq!(animation.current, 1);
    }

    #[test]
    fn tick_catches_up_over_multiple_intervals() {
        let mut animation = animation_with_frame_time();
        animation.current = 0;
        animation.last_tick =
            Instant::now() - animation.frame_time * 2 - Duration::from_millis(200);

        animation.tick();

        assert_eq!(animation.current, 2);

        let current = animation.current;
        animation.tick();

        assert_eq!(animation.current, current);
    }

    #[test]
    fn tick_wraps_around_frame_sequence() {
        let mut animation = animation_with_frame_time();
        animation.current = animation.len() - 1;
        animation.last_tick = Instant::now() - animation.frame_time;

        animation.tick();

        assert_eq!(animation.current, 0);
    }

    #[test]
    fn high_fps_uses_sub_millisecond_frame_time() {
        let animation = SpriteAnimation::from_png_bytes(
            include_bytes!("../../assets/spirit/slime/slime_idle.png"),
            4,
            2000,
        )
        .expect("slime idle sprite sheet should decode into frames");

        assert_eq!(animation.frame_time, Duration::from_nanos(500_000));
        assert!(animation.frame_time > Duration::ZERO);
    }

    #[test]
    fn to_plain_string_renders_glyphs_without_ansi() {
        let frame = TerminalFrame {
            rows: vec![
                vec![
                    TerminalCell {
                        glyph: '▀',
                        fg: Some(Rgb(255, 0, 0)),
                        bg: None,
                    },
                    TerminalCell {
                        glyph: '█',
                        fg: Some(Rgb(0, 255, 0)),
                        bg: None,
                    },
                ],
                vec![
                    TerminalCell {
                        glyph: ' ',
                        fg: None,
                        bg: None,
                    },
                    TerminalCell {
                        glyph: '▄',
                        fg: Some(Rgb(0, 0, 255)),
                        bg: Some(Rgb(128, 128, 128)),
                    },
                ],
            ],
        };

        let plain = frame.to_plain_string();
        assert_eq!(plain, "▀█\n ▄");
        assert!(!plain.contains("\u{1b}"));
    }

    #[test]
    fn is_empty_returns_false_for_loaded_animation() {
        let animation = animation_with_frame_time();
        assert!(!animation.is_empty());
    }

    #[test]
    fn frame_returns_current_frame() {
        let animation = animation_with_frame_time();
        let frame = animation.frame();
        assert!(!frame.rows.is_empty());
    }

    #[test]
    fn from_png_bytes_rejects_zero_frame_count() {
        let result = SpriteAnimation::from_png_bytes(
            include_bytes!("../../assets/spirit/slime/slime_idle.png"),
            0,
            8,
        );
        assert!(matches!(result, Err(SpriteError::InvalidFrameCount)));
    }

    #[test]
    fn from_png_bytes_rejects_zero_fps() {
        let result = SpriteAnimation::from_png_bytes(
            include_bytes!("../../assets/spirit/slime/slime_idle.png"),
            4,
            0,
        );
        assert!(matches!(result, Err(SpriteError::InvalidFps)));
    }

    #[test]
    fn from_png_bytes_rejects_indivisible_width() {
        let result = SpriteAnimation::from_png_bytes(
            include_bytes!("../../assets/spirit/slime/slime_idle.png"),
            3,
            8,
        );
        assert!(matches!(result, Err(SpriteError::WidthNotDivisible { .. })));
    }

    #[test]
    fn sprite_error_display_formats_all_variants() {
        assert_eq!(
            SpriteError::InvalidFrameCount.to_string(),
            "frame_count must be greater than 0"
        );
        assert_eq!(
            SpriteError::InvalidFps.to_string(),
            "fps must be greater than 0"
        );
        let wnd = SpriteError::WidthNotDivisible {
            width: 100,
            frame_count: 3,
        };
        assert!(
            wnd.to_string()
                .contains("100 is not divisible by frame count 3")
        );
    }

    #[test]
    fn sprite_error_source_returns_none_for_non_decode_variants() {
        use std::error::Error;
        assert!(SpriteError::InvalidFrameCount.source().is_none());
        assert!(SpriteError::InvalidFps.source().is_none());
    }

    #[test]
    fn sprite_error_from_image_error() {
        let bad_bytes = b"not a png";
        let img_err = image::load_from_memory(bad_bytes).unwrap_err();
        let sprite_err = SpriteError::from(img_err);
        assert!(matches!(sprite_err, SpriteError::Decode(_)));

        use std::error::Error;
        assert!(sprite_err.source().is_some());
        assert!(sprite_err.to_string().contains("failed to decode"));
    }
}
