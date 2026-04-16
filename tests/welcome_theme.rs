use blazar::welcome::theme::{MASCOT_ALIAS_ZH, MASCOT_NAME, MASCOT_PALETTE, MascotPalette};

#[test]
fn mascot_palette_matches_approved_design() {
    assert_eq!(MASCOT_NAME, "Lightcore Emissary");
    assert_eq!(MASCOT_ALIAS_ZH, "光核使者");
    assert_eq!(
        MASCOT_PALETTE,
        MascotPalette {
            base_hex: "#0B1020",
            visor_hex: "#B8F7FF",
            core_hex: "#FFB45E",
            base_share_percent: 80,
            accent_share_percent: 15,
            core_share_percent: 5,
        }
    );
}
