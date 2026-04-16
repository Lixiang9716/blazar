#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MascotPalette {
    pub base_hex: &'static str,
    pub visor_hex: &'static str,
    pub core_hex: &'static str,
    pub base_share_percent: u8,
    pub accent_share_percent: u8,
    pub core_share_percent: u8,
}

pub const MASCOT_NAME: &str = "Lightcore Emissary";
pub const MASCOT_ALIAS_ZH: &str = "光核使者";

pub const MASCOT_PALETTE: MascotPalette = MascotPalette {
    base_hex: "#0B1020",
    visor_hex: "#B8F7FF",
    core_hex: "#FFB45E",
    base_share_percent: 80,
    accent_share_percent: 15,
    core_share_percent: 5,
};
