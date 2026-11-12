use crate::document::Rgb;

mod presets;

pub use presets::{
    CATPPUCCIN_LATTE, CATPPUCCIN_MOCHA, DRACULA, GITHUB_DARK, GITHUB_LIGHT, GRUVBOX_DARK, GRUVBOX_LIGHT, MRK_DARK, MRK_LIGHT, NORD,
    TOKYO_NIGHT,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
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

impl Palette {
    /// Every colour of the palette, in the order the fields are declared.
    pub fn colors(self) -> impl Iterator<Item = Rgb> {
        let Self { text, muted, subtle, surface, accent, h1, h2, h3, link, code, success, note, tip, important, warning, caution } = self;
        [text, muted, subtle, surface, accent, h1, h2, h3, link, code, success, note, tip, important, warning, caution].into_iter()
    }
}

/// The syntax themes the presets use; `code` maps each onto the one embedded in two-face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SyntaxTheme {
    CatppuccinLatte,
    CatppuccinMacchiato,
    CatppuccinMocha,
    Dracula,
    GitHub,
    GruvboxDark,
    GruvboxLight,
    Nord,
    OneHalfDark,
    OneHalfLight,
    TwoDark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Theme {
    pub name: &'static str,
    pub appearance: Appearance,
    pub palette: Palette,
    pub syntax: SyntaxTheme,
}

/// Every built-in theme name, defaults first.
pub fn names() -> impl Iterator<Item = &'static str> {
    presets::ALL.iter().map(|theme| theme.name)
}

/// The built-in theme called `name`, ignoring case.
pub fn find(name: &str) -> Option<Theme> {
    presets::ALL.iter().find(|theme| theme.name.eq_ignore_ascii_case(name.trim())).copied()
}

pub fn default_for(appearance: Appearance) -> Theme {
    match appearance {
        Appearance::Dark => MRK_DARK,
        Appearance::Light => MRK_LIGHT,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn relative_luminance(color: Rgb) -> f64 {
        let channel = |value: u8| {
            let value = f64::from(value) / 255.0;
            if value <= 0.039_28 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(color.0) + 0.7152 * channel(color.1) + 0.0722 * channel(color.2)
    }

    fn contrast(left: Rgb, right: Rgb) -> f64 {
        let (left, right) = (relative_luminance(left), relative_luminance(right));
        (left.max(right) + 0.05) / (left.min(right) + 0.05)
    }

    fn all() -> impl Iterator<Item = Theme> {
        names().map(|name| find(name).unwrap())
    }

    #[test]
    fn every_requested_preset_exists() {
        let expected = [
            "mrk-dark",
            "mrk-light",
            "catppuccin-mocha",
            "catppuccin-latte",
            "tokyo-night",
            "nord",
            "dracula",
            "gruvbox-dark",
            "gruvbox-light",
            "github-dark",
            "github-light",
        ];

        assert_eq!(names().collect::<Vec<_>>(), expected);
    }

    #[test]
    fn text_is_readable_on_surface() {
        for theme in all() {
            let ratio = contrast(theme.palette.text, theme.palette.surface);
            assert!(ratio >= 4.5, "{}: text on surface is {ratio:.2}:1", theme.name);
        }
    }

    #[test]
    fn muted_stays_readable_and_subtle_stays_quiet() {
        for theme in all() {
            let Palette { text, muted, subtle, surface, .. } = theme.palette;
            let (text, muted, subtle) = (contrast(text, surface), contrast(muted, surface), contrast(subtle, surface));
            assert!(muted >= 3.5, "{}: muted on surface is {muted:.2}:1", theme.name);
            assert!((1.2..2.5).contains(&subtle), "{}: subtle on surface is {subtle:.2}:1", theme.name);
            assert!(text > muted && muted > subtle, "{}: text, muted, subtle must fade in order", theme.name);
        }
    }

    #[test]
    fn appearance_matches_the_surface_lightness() {
        for theme in all() {
            let is_light_surface = relative_luminance(theme.palette.surface) > 0.5;
            assert_eq!(is_light_surface, theme.appearance == Appearance::Light, "{}", theme.name);
        }
    }

    #[test]
    fn find_ignores_case() {
        assert_eq!(find("Catppuccin-Mocha").map(|theme| theme.name), Some("catppuccin-mocha"));
        assert_eq!(find("nope"), None);
    }
}
