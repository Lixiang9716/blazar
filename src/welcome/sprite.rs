use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    time::{Duration, Instant},
};

use image::{Rgba, RgbaImage};

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
            frame_time: Duration::from_millis(1000 / u64::from(fps)),
            last_tick: Instant::now(),
        })
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn frame_by_index(&self, index: usize) -> &TerminalFrame {
        &self.frames[index % self.frames.len()]
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

fn build_frame(sheet: &RgbaImage, x_offset: u32, frame_width: u32, frame_height: u32) -> TerminalFrame {
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
