use std::fmt;

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

/// The syntax themes the presets use; `code` maps each onto the one embedded in two-face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
pub struct Theme {
    pub name: &'static str,
    pub appearance: Appearance,
    pub palette: Palette,
    pub syntax: SyntaxTheme,
}

const MAX_SUGGESTION_DISTANCE: usize = 3;
/// A suggestion allows one edit per this many typed characters, so a short name is only matched by a near twin.
const CHARS_PER_EDIT: usize = 3;

/// A theme name that is not built in, with the closest built-in name when one is near enough to be a typo.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct UnknownTheme {
    pub name: String,
    pub suggestion: Option<&'static str>,
}

impl UnknownTheme {
    fn new(name: &str) -> Self {
        Self { name: name.to_owned(), suggestion: closest_name(name) }
    }
}

impl fmt::Display for UnknownTheme {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { name, suggestion } = self;
        let themes = names().collect::<Vec<_>>().join(", ");
        match suggestion {
            Some(suggestion) => write!(formatter, "unknown theme {name:?}, did you mean \"{suggestion}\"? Themes: {themes}"),
            None => write!(formatter, "unknown theme {name:?}. Themes: {themes}"),
        }
    }
}

impl std::error::Error for UnknownTheme {}

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

/// The theme to draw with: one asked for by name, or one for each terminal background.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    Named(Theme),
    ByBackground { dark: Theme, light: Theme },
}

impl Choice {
    /// `name` wins over the pair; a side of the pair left out is the built-in default. Every name given must exist.
    pub fn new(name: Option<&str>, dark: Option<&str>, light: Option<&str>) -> Result<Self, UnknownTheme> {
        let named_theme = name.map(named).transpose()?;
        let dark = named_or_default(dark, Appearance::Dark)?;
        let light = named_or_default(light, Appearance::Light)?;
        Ok(named_theme.map_or(Self::ByBackground { dark, light }, Self::Named))
    }

    pub fn needs_background(&self) -> bool {
        matches!(self, Self::ByBackground { .. })
    }

    /// The theme for the terminal background, dark when it is unknown.
    pub fn resolve(self, background: Option<Appearance>) -> Theme {
        match self {
            Self::Named(theme) => theme,
            Self::ByBackground { dark, light } => match background.unwrap_or(Appearance::Dark) {
                Appearance::Dark => dark,
                Appearance::Light => light,
            },
        }
    }
}

fn named(name: &str) -> Result<Theme, UnknownTheme> {
    find(name).ok_or_else(|| UnknownTheme::new(name))
}

fn named_or_default(name: Option<&str>, appearance: Appearance) -> Result<Theme, UnknownTheme> {
    name.map_or_else(|| Ok(default_for(appearance)), named)
}

fn closest_name(name: &str) -> Option<&'static str> {
    let wanted = name.trim().to_ascii_lowercase();
    let allowed = (wanted.chars().count() / CHARS_PER_EDIT).clamp(1, MAX_SUGGESTION_DISTANCE);
    names()
        .map(|candidate| (edit_distance(&wanted, candidate), candidate))
        .filter(|(distance, _)| *distance <= allowed)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

/// Insertions, deletions, substitutions and swaps of two neighbours, each one edit: a swap is the commonest typo.
fn edit_distance(left: &str, right: &str) -> usize {
    let (left, right): (Vec<char>, Vec<char>) = (left.chars().collect(), right.chars().collect());
    let mut distances = vec![vec![0; right.len() + 1]; left.len() + 1];
    for (row, distance) in distances.iter_mut().enumerate() {
        distance[0] = row;
    }
    for (column, distance) in distances[0].iter_mut().enumerate() {
        *distance = column;
    }
    for row in 1..=left.len() {
        for column in 1..=right.len() {
            let substitution = distances[row - 1][column - 1] + usize::from(left[row - 1] != right[column - 1]);
            let edit = substitution.min(distances[row - 1][column] + 1).min(distances[row][column - 1] + 1);
            let is_swap = row > 1 && column > 1 && left[row - 1] == right[column - 2] && left[row - 2] == right[column - 1];
            distances[row][column] = if is_swap { edit.min(distances[row - 2][column - 2] + 1) } else { edit };
        }
    }
    distances[left.len()][right.len()]
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

    fn resolved(name: Option<&str>, dark: Option<&str>, light: Option<&str>, background: Option<Appearance>) -> &'static str {
        Choice::new(name, dark, light).unwrap().resolve(background).name
    }

    #[test]
    fn without_any_name_the_default_follows_the_background() {
        assert_eq!(resolved(None, None, None, Some(Appearance::Light)), "mrk-light");
        assert_eq!(resolved(None, None, None, Some(Appearance::Dark)), "mrk-dark");
        assert_eq!(resolved(None, None, None, None), "mrk-dark");
    }

    #[test]
    fn the_name_wins_over_the_pair_and_the_background() {
        assert_eq!(resolved(Some("NORD"), Some("dracula"), Some("github-light"), Some(Appearance::Light)), "nord");
        assert!(!Choice::new(Some("nord"), None, None).unwrap().needs_background());
    }

    #[test]
    fn the_pair_follows_the_background_and_is_dark_when_it_is_unknown() {
        let (dark, light) = (Some("dracula"), Some("github-light"));

        assert_eq!(resolved(None, dark, light, Some(Appearance::Light)), "github-light");
        assert_eq!(resolved(None, dark, light, Some(Appearance::Dark)), "dracula");
        assert_eq!(resolved(None, dark, light, None), "dracula");
        assert!(Choice::new(None, dark, light).unwrap().needs_background());
    }

    #[test]
    fn a_side_of_the_pair_left_out_is_the_default() {
        assert_eq!(resolved(None, Some("dracula"), None, Some(Appearance::Light)), "mrk-light");
        assert_eq!(resolved(None, None, Some("github-light"), Some(Appearance::Dark)), "mrk-dark");
    }

    #[test]
    fn an_unknown_name_in_the_pair_is_refused_whatever_the_background() {
        let message = Choice::new(None, Some("nord"), Some("nope")).unwrap_err().to_string();

        assert!(message.contains("unknown theme \"nope\"") && message.contains("mrk-dark"), "{message}");
    }

    #[test]
    fn an_unknown_name_in_the_pair_is_refused_even_when_a_name_wins() {
        assert_eq!(Choice::new(Some("nord"), Some("nope"), None).unwrap_err().name, "nope");
        assert_eq!(Choice::new(Some("nord"), None, Some("nope")).unwrap_err().name, "nope");
    }

    #[test]
    fn unknown_name_suggests_the_closest_and_lists_all() {
        let error = named("dracla").unwrap_err();
        let message = error.to_string();

        assert_eq!(error, UnknownTheme { name: "dracla".to_owned(), suggestion: Some("dracula") });
        assert!(message.contains("did you mean \"dracula\"?"), "{message}");
        assert!(message.contains("github-light"), "{message}");
    }

    #[test]
    fn a_suggestion_allows_one_edit_per_three_typed_characters_and_at_least_one() {
        assert_eq!(closest_name("nordd"), Some("nord"));
        assert_eq!(closest_name("nrod"), Some("nord"));
        assert_eq!(closest_name("nrdo"), None);
        assert_eq!(closest_name("bad"), None);
        assert_eq!(closest_name("x"), None);
    }

    #[test]
    fn far_name_lists_themes_without_a_guess() {
        let message = named("solarized").unwrap_err().to_string();

        assert!(!message.contains("did you mean"), "{message}");
        assert!(message.contains("mrk-dark, mrk-light"), "{message}");
    }

    #[test]
    fn edit_distance_counts_edits() {
        assert_eq!(edit_distance("nord", "nord"), 0);
        assert_eq!(edit_distance("nrod", "nord"), 1);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("", "abc"), 3);
    }
}
