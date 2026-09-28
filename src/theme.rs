use crate::document::Rgb;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub text: Rgb,
    pub muted: Rgb,
    pub subtle: Rgb,
    pub surface: Rgb,
    pub accent: Rgb,
    pub h1: Rgb,
    pub h2: Rgb,
    pub h3: Rgb,
    pub link: Rgb,
    pub code: Rgb,
    pub success: Rgb,
    pub note: Rgb,
    pub tip: Rgb,
    pub important: Rgb,
    pub warning: Rgb,
    pub caution: Rgb,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    pub appearance: Appearance,
    pub palette: Palette,
    /// A theme name from `two_face::theme::EmbeddedThemeName`, as syntect knows it.
    pub syntax: &'static str,
}

pub const MRK_DARK: Theme = Theme {
    name: "mrk-dark",
    appearance: Appearance::Dark,
    palette: Palette {
        text: Rgb(0xd4, 0xd7, 0xe0),
        muted: Rgb(0x8b, 0x91, 0xa3),
        subtle: Rgb(0x3e, 0x44, 0x55),
        surface: Rgb(0x1f, 0x23, 0x2e),
        accent: Rgb(0x8a, 0xb4, 0xf8),
        h1: Rgb(0xc5, 0x94, 0xf7),
        h2: Rgb(0x8a, 0xb4, 0xf8),
        h3: Rgb(0x6f, 0xd3, 0xc6),
        link: Rgb(0x7a, 0xc8, 0xf5),
        code: Rgb(0xf2, 0xb4, 0x82),
        success: Rgb(0x8f, 0xd6, 0x94),
        note: Rgb(0x8a, 0xb4, 0xf8),
        tip: Rgb(0x8f, 0xd6, 0x94),
        important: Rgb(0xc5, 0x94, 0xf7),
        warning: Rgb(0xf0, 0xc6, 0x74),
        caution: Rgb(0xf2, 0x8b, 0x8b),
    },
    syntax: "OneHalfDark",
};

// TEMPORARY shim from feat/terminal so the CLI compiles: feat/code owns the real preset list, `names` and `find`.
const PRESETS: [Theme; 1] = [MRK_DARK];

/// TEMPORARY shim (feat/terminal), replaced by feat/code.
pub fn names() -> Vec<&'static str> {
    PRESETS.iter().map(|theme| theme.name).collect()
}

/// TEMPORARY shim (feat/terminal), replaced by feat/code.
pub fn find(name: &str) -> Option<Theme> {
    PRESETS.iter().find(|theme| theme.name == name).cloned()
}

pub const DEFAULT_WIDTH: usize = 100;

/// The settings unit tests render with: dark theme, 80 columns, no pictures.
pub fn test_settings() -> crate::document::Settings {
    crate::document::Settings { width: 80, theme: MRK_DARK, cell: None }
}
