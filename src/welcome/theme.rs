#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MascotPalette {
    pub coat_ansi: &'static str,
    pub pink_ansi: &'static str,
    pub mint_ansi: &'static str,
    pub blue_ansi: &'static str,
    pub horn_ansi: &'static str,
    pub star_ansi: &'static str,
    pub reset_ansi: &'static str,
}

pub const MASCOT_NAME: &str = "Star Sugar Guidepony";
pub const MASCOT_ALIAS_ZH: &str = "星糖导航马";

pub const MASCOT_PALETTE: MascotPalette = MascotPalette {
    coat_ansi: "\u{1b}[38;5;230m",
    pink_ansi: "\u{1b}[38;5;218m",
    mint_ansi: "\u{1b}[38;5;159m",
    blue_ansi: "\u{1b}[38;5;153m",
    horn_ansi: "\u{1b}[38;5;223m",
    star_ansi: "\u{1b}[38;5;222m",
    reset_ansi: "\u{1b}[0m",
};

pub fn paint(text: &str, ansi: &str) -> String {
    format!("{}{}{}", ansi, text, MASCOT_PALETTE.reset_ansi)
}
