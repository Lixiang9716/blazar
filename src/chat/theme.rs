//! Theme engine — loads opaline themes and maps semantic tokens to ChatTheme.

use opaline::{OpalineColor, ThemeInfo, list_available_themes, load_by_name};
use ratatui_core::style::{Color, Modifier, Style};
use termimad::crossterm::style::Color as CrossColor;
use termimad::{CompoundStyle, MadSkin, StyledChar};

/// Default theme name.
pub const DEFAULT_THEME: &str = "one-dark";

/// Convert an opaline color to ratatui Color.
fn to_color(c: OpalineColor) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// ChatTheme — rendering contract. Views use this, never opaline directly.
#[derive(Clone)]
pub struct ChatTheme {
    pub title_bar: Style,
    pub title_text: Style,
    pub timeline_bg: Style,
    pub body_text: Style,
    pub dim_text: Style,
    pub bold_text: Style,
    pub marker_response: Style,
    pub marker_tool: Style,
    pub marker_bash: Style,
    pub marker_thinking: Style,
    pub marker_warning: Style,
    pub marker_hint: Style,
    pub tool_label: Style,
    pub tool_target: Style,
    pub diff_add: Style,
    pub diff_del: Style,
    pub code_block: Style,
    pub input_prompt: Style,
    pub input_placeholder: Style,
    pub status_bar: Style,
    pub status_right: Style,
    pub picker_title: Style,
    pub picker_item: Style,
    pub picker_selected: Style,
    pub picker_desc: Style,
    pub spinner: Style,
    pub tip_command: Style,
    /// Background color for code blocks.
    pub code_bg: Color,
    /// Background color for overlays (picker backdrop etc.).
    pub backdrop_color: Color,
    /// termimad skin for ratskin markdown rendering.
    pub mad_skin: MadSkin,
}

/// Build `ChatTheme` using the default theme.
pub fn build_theme() -> ChatTheme {
    build_theme_by_name(DEFAULT_THEME)
}

/// Build `ChatTheme` from an opaline theme by name, falling back to Solarized Dark.
pub fn build_theme_by_name(name: &str) -> ChatTheme {
    match load_by_name(name) {
        Some(theme) => map_opaline_theme(&theme),
        None => fallback_theme(),
    }
}

/// List all available theme names for the picker UI.
pub fn available_themes() -> Vec<ThemeInfo> {
    list_available_themes()
}

fn map_opaline_theme(theme: &opaline::Theme) -> ChatTheme {
    let bg_base = to_color(theme.color("bg.base"));
    let text_primary = to_color(theme.color("text.primary"));
    let text_secondary = to_color(theme.color("text.secondary"));
    let text_muted = to_color(theme.color("text.muted"));
    let accent_primary = to_color(theme.color("accent.primary"));
    let accent_info = to_color(theme.color("accent.info"));
    let accent_warning = to_color(theme.color("accent.warning"));
    let accent_error = to_color(theme.color("accent.error"));
    let accent_success = to_color(theme.color("accent.success"));
    let code_plain = to_color(theme.color("code.plain"));
    let code_bg = to_color(theme.color("code.background"));

    let mad_skin = build_mad_skin(
        theme.color("text.primary"),
        theme.color("text.muted"),
        theme.color("accent.info"),
        theme.color("accent.primary"),
        theme.color("code.plain"),
        theme.color("code.background"),
    );

    ChatTheme {
        title_bar: Style::default().fg(text_secondary),
        title_text: Style::default()
            .fg(text_primary)
            .add_modifier(Modifier::BOLD),
        timeline_bg: Style::default().fg(text_primary),
        body_text: Style::default().fg(text_primary),
        dim_text: Style::default().fg(text_muted),
        bold_text: Style::default()
            .fg(text_primary)
            .add_modifier(Modifier::BOLD),
        marker_response: Style::default().fg(accent_primary),
        marker_tool: Style::default().fg(accent_success),
        marker_bash: Style::default().fg(accent_success),
        marker_thinking: Style::default().fg(accent_warning),
        marker_warning: Style::default().fg(accent_error),
        marker_hint: Style::default().fg(accent_warning),
        tool_label: Style::default()
            .fg(text_primary)
            .add_modifier(Modifier::BOLD),
        tool_target: Style::default().fg(accent_info),
        diff_add: Style::default().fg(accent_success),
        diff_del: Style::default().fg(accent_error),
        code_block: Style::default().fg(code_plain),
        input_prompt: Style::default().fg(accent_info),
        input_placeholder: Style::default().fg(text_muted),
        status_bar: Style::default().fg(text_secondary),
        status_right: Style::default().fg(text_muted),
        picker_title: Style::default()
            .fg(accent_primary)
            .add_modifier(Modifier::BOLD),
        picker_item: Style::default().fg(text_primary),
        picker_selected: Style::default()
            .fg(accent_info)
            .add_modifier(Modifier::BOLD),
        picker_desc: Style::default().fg(text_muted),
        spinner: Style::default().fg(accent_info),
        tip_command: Style::default().fg(accent_warning),
        code_bg,
        backdrop_color: bg_base,
        mad_skin,
    }
}

/// Solarized Dark fallback — used when opaline fails to load a theme.
fn fallback_theme() -> ChatTheme {
    // Solarized Dark palette
    const BASE03: Color = Color::Rgb(0, 43, 54);
    const BASE02: Color = Color::Rgb(7, 54, 66);
    const BASE01: Color = Color::Rgb(88, 110, 117);
    const BASE0: Color = Color::Rgb(131, 148, 150);
    const BASE1: Color = Color::Rgb(147, 161, 161);
    const BASE2: Color = Color::Rgb(238, 232, 213);
    const YELLOW: Color = Color::Rgb(181, 137, 0);
    const RED: Color = Color::Rgb(220, 50, 47);
    const BLUE: Color = Color::Rgb(38, 139, 210);
    const CYAN: Color = Color::Rgb(42, 161, 152);
    const GREEN: Color = Color::Rgb(133, 153, 0);

    ChatTheme {
        title_bar: Style::default().fg(BASE1),
        title_text: Style::default().fg(BASE2).add_modifier(Modifier::BOLD),
        timeline_bg: Style::default().fg(BASE0),
        body_text: Style::default().fg(BASE0),
        dim_text: Style::default().fg(BASE01),
        bold_text: Style::default().fg(BASE1).add_modifier(Modifier::BOLD),
        marker_response: Style::default().fg(YELLOW),
        marker_tool: Style::default().fg(GREEN),
        marker_bash: Style::default().fg(GREEN),
        marker_thinking: Style::default().fg(YELLOW),
        marker_warning: Style::default().fg(RED),
        marker_hint: Style::default().fg(YELLOW),
        tool_label: Style::default().fg(BASE2).add_modifier(Modifier::BOLD),
        tool_target: Style::default().fg(CYAN),
        diff_add: Style::default().fg(GREEN),
        diff_del: Style::default().fg(RED),
        code_block: Style::default().fg(BASE0),
        input_prompt: Style::default().fg(CYAN),
        input_placeholder: Style::default().fg(BASE01),
        status_bar: Style::default().fg(BASE1),
        status_right: Style::default().fg(BASE01),
        picker_title: Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        picker_item: Style::default().fg(BASE0),
        picker_selected: Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        picker_desc: Style::default().fg(BASE01),
        spinner: Style::default().fg(CYAN),
        tip_command: Style::default().fg(YELLOW),
        code_bg: BASE02,
        backdrop_color: BASE03,
        mad_skin: build_mad_skin(
            OpalineColor {
                r: 131,
                g: 148,
                b: 150,
            }, // BASE0
            OpalineColor {
                r: 88,
                g: 110,
                b: 117,
            }, // BASE01
            OpalineColor {
                r: 42,
                g: 161,
                b: 152,
            }, // CYAN
            OpalineColor {
                r: 38,
                g: 139,
                b: 210,
            }, // BLUE
            OpalineColor {
                r: 131,
                g: 148,
                b: 150,
            }, // BASE0
            OpalineColor { r: 7, g: 54, b: 66 }, // BASE02
        ),
    }
}

/// Convert an OpalineColor to a crossterm Color (used by termimad).
fn to_cross(c: OpalineColor) -> CrossColor {
    CrossColor::Rgb {
        r: c.r,
        g: c.g,
        b: c.b,
    }
}

/// Build a `MadSkin` for ratskin markdown rendering, themed to match
/// the current Blazar palette.
fn build_mad_skin(
    text: OpalineColor,
    muted: OpalineColor,
    accent: OpalineColor,
    heading: OpalineColor,
    code_fg: OpalineColor,
    code_bg: OpalineColor,
) -> MadSkin {
    use termimad::crossterm::style::Attribute;
    use termimad::minimad::Alignment;

    let mut skin = MadSkin::default();

    // Paragraph / body text
    skin.paragraph.compound_style.set_fg(to_cross(text));

    // Bold — bright white, stands out
    skin.bold = CompoundStyle::new(Some(CrossColor::White), None, Attribute::Bold.into());

    // Italic
    skin.italic = CompoundStyle::new(Some(to_cross(text)), None, Attribute::Italic.into());

    // Inline code — code fg on code bg
    skin.inline_code
        .set_fgbg(to_cross(code_fg), to_cross(code_bg));

    // Code blocks — same colors, no underline
    skin.code_block
        .compound_style
        .set_fgbg(to_cross(code_fg), to_cross(code_bg));

    // Headers — accent colored, bold, left-aligned (not centered)
    for (i, h) in skin.headers.iter_mut().enumerate() {
        h.compound_style =
            CompoundStyle::new(Some(to_cross(heading)), None, Attribute::Bold.into());
        h.align = Alignment::Left;
        // H1 gets underline for emphasis
        if i == 0 {
            h.add_attr(Attribute::Underlined);
        }
    }

    // Bullet — accent colored dot
    skin.bullet = StyledChar::new(
        CompoundStyle::new(Some(to_cross(accent)), None, Default::default()),
        '•',
    );

    // Quote mark
    skin.quote_mark = StyledChar::new(
        CompoundStyle::new(Some(to_cross(muted)), None, Attribute::Bold.into()),
        '▐',
    );

    // Horizontal rule — muted thin line
    skin.horizontal_rule = StyledChar::new(
        CompoundStyle::new(Some(to_cross(muted)), None, Default::default()),
        '─',
    );

    // Table borders — muted color
    skin.table.compound_style.set_fg(to_cross(muted));

    skin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_loads_successfully() {
        let theme = build_theme();
        assert_ne!(theme.body_text, Style::default());
    }

    #[test]
    fn fallback_works_for_invalid_name() {
        let theme = build_theme_by_name("nonexistent-theme-xyz");
        assert_ne!(theme.body_text, Style::default());
    }

    #[test]
    fn available_themes_returns_nonempty() {
        assert!(!available_themes().is_empty());
    }
}
