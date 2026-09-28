use anyhow::anyhow;

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

pub const DEFAULT_WIDTH: usize = 100;

const MAX_SUGGESTION_DISTANCE: usize = 3;

/// The settings unit tests render with: dark theme, 80 columns, no pictures.
pub fn test_settings() -> crate::document::Settings {
    crate::document::Settings { width: 80, theme: MRK_DARK, cell: None }
}

/// Every built-in theme name, defaults first.
pub fn names() -> impl Iterator<Item = &'static str> {
    presets::ALL.iter().map(|theme| theme.name)
}

/// The built-in theme called `name`, ignoring case.
pub fn find(name: &str) -> Option<Theme> {
    presets::ALL.iter().find(|theme| theme.name.eq_ignore_ascii_case(name.trim())).cloned()
}

pub fn default_for(appearance: Appearance) -> Theme {
    match appearance {
        Appearance::Dark => MRK_DARK,
        Appearance::Light => MRK_LIGHT,
    }
}

/// The theme asked for by name, or the default matching the terminal background (dark when it is unknown).
pub fn resolve(requested: Option<&str>, background: Option<Appearance>) -> anyhow::Result<Theme> {
    match requested {
        Some(name) => find(name).ok_or_else(|| unknown_theme(name)),
        None => Ok(default_for(background.unwrap_or(Appearance::Dark))),
    }
}

fn unknown_theme(name: &str) -> anyhow::Error {
    let valid = names().collect::<Vec<_>>().join(", ");
    match closest_name(name) {
        Some(suggestion) => anyhow!("unknown theme \"{name}\", did you mean \"{suggestion}\"? Themes: {valid}"),
        None => anyhow!("unknown theme \"{name}\". Themes: {valid}"),
    }
}

fn closest_name(name: &str) -> Option<&'static str> {
    let wanted = name.trim().to_ascii_lowercase();
    names()
        .map(|candidate| (distance(&wanted, candidate), candidate))
        .filter(|(distance, _)| *distance <= MAX_SUGGESTION_DISTANCE)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

/// Levenshtein distance, counted in chars.
fn distance(left: &str, right: &str) -> usize {
    let right: Vec<char> = right.chars().collect();
    let first_row: Vec<usize> = (0..=right.len()).collect();
    let last_row = left.chars().enumerate().fold(first_row, |previous, (row, left_char)| {
        right.iter().enumerate().fold(vec![row + 1], |mut current, (column, right_char)| {
            let substitution = previous[column] + usize::from(left_char != *right_char);
            let cost = substitution.min(previous[column + 1] + 1).min(current[column] + 1);
            current.push(cost);
            current
        })
    });
    last_row[right.len()]
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
    fn every_syntax_theme_is_embedded_in_two_face() {
        let embedded: Vec<&str> = two_face::theme::EmbeddedLazyThemeSet::theme_names().iter().map(|name| name.as_name()).collect();

        for theme in all() {
            assert!(embedded.contains(&theme.syntax), "{} uses missing syntax theme {}", theme.name, theme.syntax);
        }
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

    #[test]
    fn resolve_without_a_name_follows_the_background() {
        assert_eq!(resolve(None, Some(Appearance::Light)).unwrap().name, "mrk-light");
        assert_eq!(resolve(None, Some(Appearance::Dark)).unwrap().name, "mrk-dark");
        assert_eq!(resolve(None, None).unwrap().name, "mrk-dark");
    }

    #[test]
    fn resolve_prefers_the_name_over_the_background() {
        assert_eq!(resolve(Some("NORD"), Some(Appearance::Light)).unwrap().name, "nord");
    }

    #[test]
    fn unknown_name_suggests_the_closest_and_lists_all() {
        let message = resolve(Some("dracla"), None).unwrap_err().to_string();

        assert!(message.contains("did you mean \"dracula\"?"), "{message}");
        assert!(message.contains("github-light"), "{message}");
    }

    #[test]
    fn far_name_lists_themes_without_a_guess() {
        let message = resolve(Some("solarized"), None).unwrap_err().to_string();

        assert!(!message.contains("did you mean"), "{message}");
        assert!(message.contains("mrk-dark, mrk-light"), "{message}");
    }

    #[test]
    fn distance_counts_edits() {
        assert_eq!(distance("nord", "nord"), 0);
        assert_eq!(distance("nrod", "nord"), 2);
        assert_eq!(distance("", "abc"), 3);
    }
}
