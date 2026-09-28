use crossterm::terminal::WindowSize;

use super::environment::{Environment, background_hint, color_depth, hyperlinks, known_graphics, wants_pictures};
use super::query::{self, Questions};
use super::reply::Replies;
use super::{Capabilities, ColorChoice, ColorDepth, ImagesMode};
use crate::document::{CellSize, Rgb};
use crate::theme::Appearance;

const DEFAULT_COLUMNS: u16 = 80;

/// What the user asked for, which detection must respect.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Preferences {
    pub color: ColorChoice,
    pub images: ImagesMode,
    /// The theme was not named, so the background decides between dark and light.
    pub needs_background: bool,
}

pub fn cell_size(window: &WindowSize) -> Option<CellSize> {
    let has_pixels = window.width > 0 && window.height > 0;
    let has_cells = window.columns > 0 && window.rows > 0;
    let cell = CellSize { width_px: window.width / window.columns.max(1), height_px: window.height / window.rows.max(1) };
    let is_usable = cell.width_px > 0 && cell.height_px > 0;
    (has_pixels && has_cells && is_usable).then_some(cell)
}

pub fn appearance(background: Rgb) -> Appearance {
    let luminance = 2126 * u32::from(background.0) + 7152 * u32::from(background.1) + 722 * u32::from(background.2);
    let is_dark = luminance < 10_000 * 128;
    if is_dark { Appearance::Dark } else { Appearance::Light }
}

fn questions(environment: &Environment, color: ColorDepth, cell: Option<CellSize>, preferences: Preferences) -> Questions {
    let is_background_unknown = preferences.needs_background && background_hint(environment).is_none();
    Questions {
        graphics: cell.is_some() && known_graphics(environment).is_none(),
        background: is_background_unknown && environment.stdout_is_tty && color != ColorDepth::None,
    }
}

/// What the terminal can do, from the environment, the window size and, only when stdout is a terminal and the
/// environment leaves a question open, one round of terminal queries.
pub fn detect(preferences: Preferences) -> Capabilities {
    let environment = Environment::read();
    let window = crossterm::terminal::window_size().ok();
    let color = color_depth(&environment, preferences.color);
    let cell = window.as_ref().and_then(cell_size).filter(|_| wants_pictures(&environment, preferences.images));

    let questions = questions(&environment, color, cell, preferences);
    let replies = if questions.is_empty() { Replies::default() } else { query::ask(questions) };

    let has_graphics = known_graphics(&environment).unwrap_or(replies.graphics);
    let background = background_hint(&environment).or(replies.background.map(appearance));
    let columns = window.as_ref().map(|window| window.columns).filter(|&columns| columns > 0).unwrap_or(DEFAULT_COLUMNS);
    Capabilities { color, hyperlinks: hyperlinks(&environment, color), cell: cell.filter(|_| has_graphics), background, columns }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(columns: u16, rows: u16, width: u16, height: u16) -> WindowSize {
        WindowSize { rows, columns, width, height }
    }

    #[test]
    fn the_cell_size_is_pixels_over_cells() {
        assert_eq!(cell_size(&window(100, 50, 1000, 1000)), Some(CellSize { width_px: 10, height_px: 20 }));
    }

    #[test]
    fn an_unknown_pixel_size_means_no_pictures() {
        assert_eq!(cell_size(&window(100, 50, 0, 0)), None);
        assert_eq!(cell_size(&window(0, 0, 1000, 1000)), None);
        assert_eq!(cell_size(&window(100, 50, 50, 1000)), None);
    }

    #[test]
    fn luminance_splits_dark_from_light() {
        assert_eq!(appearance(Rgb(0x1e, 0x1e, 0x2e)), Appearance::Dark);
        assert_eq!(appearance(Rgb(0xff, 0xff, 0xff)), Appearance::Light);
        assert_eq!(appearance(Rgb(0xfd, 0xf6, 0xe3)), Appearance::Light);
        assert_eq!(appearance(Rgb(0x00, 0x2b, 0x36)), Appearance::Dark);
    }

    fn environment(variables: &[(&str, &str)]) -> Environment {
        let variables = variables.iter().map(|&(name, value)| (name.to_owned(), value.to_owned())).collect();
        Environment { variables, stdout_is_tty: true }
    }

    const CELL: Option<CellSize> = Some(CellSize { width_px: 10, height_px: 20 });

    #[test]
    fn nothing_is_asked_when_the_environment_knows() {
        let preferences = Preferences { needs_background: true, ..Preferences::default() };
        let known = environment(&[("TERM", "xterm-kitty"), ("COLORFGBG", "15;0")]);

        assert!(questions(&known, ColorDepth::TrueColor, CELL, preferences).is_empty());
    }

    #[test]
    fn open_questions_are_asked_together() {
        let preferences = Preferences { needs_background: true, ..Preferences::default() };
        let asked = questions(&environment(&[("TERM", "xterm-256color")]), ColorDepth::Ansi256, CELL, preferences);

        assert_eq!(asked, Questions { graphics: true, background: true });
    }

    #[test]
    fn a_named_theme_skips_the_background_question() {
        let asked = questions(&environment(&[]), ColorDepth::TrueColor, None, Preferences::default());

        assert!(asked.is_empty());
    }

    #[test]
    fn nothing_is_asked_when_stdout_is_not_a_terminal() {
        let preferences = Preferences { needs_background: true, ..Preferences::default() };
        let piped = Environment { stdout_is_tty: false, ..environment(&[]) };

        assert!(questions(&piped, ColorDepth::TrueColor, None, preferences).is_empty());
    }
}
