use blazar::welcome::theme::{
    paint, MascotPalette, MASCOT_ALIAS_ZH, MASCOT_NAME, MASCOT_PALETTE,
};

#[test]
fn unicorn_palette_matches_approved_design() {
    assert_eq!(MASCOT_NAME, "Star Sugar Guidepony");
    assert_eq!(MASCOT_ALIAS_ZH, "星糖导航马");
    assert_eq!(
        MASCOT_PALETTE,
        MascotPalette {
            coat_ansi: "\u{1b}[38;5;230m",
            pink_ansi: "\u{1b}[38;5;218m",
            mint_ansi: "\u{1b}[38;5;159m",
            blue_ansi: "\u{1b}[38;5;153m",
            horn_ansi: "\u{1b}[38;5;223m",
            star_ansi: "\u{1b}[38;5;222m",
            reset_ansi: "\u{1b}[0m",
        }
    );
}

#[test]
fn paint_wraps_segments_in_ansi_sequences() {
    assert_eq!(
        paint("★", MASCOT_PALETTE.star_ansi),
        "\u{1b}[38;5;222m★\u{1b}[0m"
    );
}
