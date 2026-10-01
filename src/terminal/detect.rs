use crossterm::terminal::WindowSize;

use super::environment::{Environment, background_hint, color_depth, hyperlinks, known_graphics, wants_pictures};
use super::query::{self, Questions};
use super::reply::Replies;
use super::{Capabilities, ColorChoice, ColorDepth, Graphics, ImagesMode, Protocol};
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

fn questions(environment: &Environment, color: ColorDepth, window_cell: Option<CellSize>, preferences: Preferences) -> Questions {
    let is_background_unknown = preferences.needs_background && background_hint(environment).is_none();
    let known = known_graphics(environment);
    let may_draw = wants_pictures(environment, preferences.images) && known != Some(false);
    Questions {
        graphics: may_draw && known.is_none(),
        cell: may_draw && window_cell.is_none(),
        background: is_background_unknown && environment.stdout_is_tty && color != ColorDepth::None,
    }
}

/// Kitty when the environment or the terminal says so, Sixel when only the DA1 reply lists it.
fn protocol(environment: &Environment, replies: &Replies) -> Option<Protocol> {
    match known_graphics(environment) {
        Some(true) => Some(Protocol::Kitty),
        Some(false) => None,
        None if replies.graphics => Some(Protocol::Kitty),
        None => replies.sixel.then_some(Protocol::Sixel),
    }
}

/// How pictures are drawn, when they are wanted, the terminal draws them, and the cell size is known from the window
/// or the terminal's reply.
fn graphics(environment: &Environment, images: ImagesMode, window_cell: Option<CellSize>, replies: &Replies) -> Option<Graphics> {
    let protocol = protocol(environment, replies).filter(|_| wants_pictures(environment, images))?;
    let cell = window_cell.or(replies.cell)?;
    Some(Graphics { protocol, cell })
}

/// What the terminal can do, from the environment, the window size and, only when stdout is a terminal and the
/// environment leaves a question open, one round of terminal queries.
pub fn detect(preferences: Preferences) -> Capabilities {
    let environment = Environment::read();
    let window = crossterm::terminal::window_size().ok();
    let color = color_depth(&environment, preferences.color);
    let window_cell = window.as_ref().and_then(cell_size);

    let questions = questions(&environment, color, window_cell, preferences);
    let replies = if questions.is_empty() { Replies::default() } else { query::ask(questions) };

    let background = background_hint(&environment).or(replies.background.map(appearance));
    let columns = window.as_ref().map(|window| window.columns).filter(|&columns| columns > 0).unwrap_or(DEFAULT_COLUMNS);
    Capabilities {
        color,
        hyperlinks: hyperlinks(&environment, color),
        graphics: graphics(&environment, preferences.images, window_cell, &replies),
        background,
        columns,
        is_terminal: environment.stdout_is_tty,
    }
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

        assert_eq!(asked, Questions { graphics: true, cell: false, background: true });
    }

    #[test]
    fn the_cell_size_is_asked_only_when_the_window_does_not_tell_it() {
        let kitty = environment(&[("TERM", "xterm-kitty")]);

        assert_eq!(
            questions(&kitty, ColorDepth::TrueColor, None, Preferences::default()),
            Questions { cell: true, ..Questions::default() }
        );
        assert!(questions(&kitty, ColorDepth::TrueColor, CELL, Preferences::default()).is_empty());
    }

    #[test]
    fn graphics_are_not_asked_about_where_pictures_are_off() {
        let never = Preferences { images: ImagesMode::Never, ..Preferences::default() };

        assert!(questions(&environment(&[]), ColorDepth::TrueColor, None, never).is_empty());
        assert!(questions(&environment(&[("TERM_PROGRAM", "iTerm.app")]), ColorDepth::TrueColor, None, Preferences::default()).is_empty());
        assert!(questions(&environment(&[("TMUX", "/tmp/tmux")]), ColorDepth::TrueColor, CELL, Preferences::default()).is_empty());
    }

    fn drawn_with(variables: &[(&str, &str)], replies: Replies) -> Option<Protocol> {
        graphics(&environment(variables), ImagesMode::Auto, CELL, &replies).map(|graphics| graphics.protocol)
    }

    #[test]
    fn kitty_wins_over_sixel() {
        assert_eq!(drawn_with(&[], Replies { graphics: true, sixel: true, ..Replies::default() }), Some(Protocol::Kitty));
        assert_eq!(drawn_with(&[("TERM", "xterm-kitty")], Replies { sixel: true, ..Replies::default() }), Some(Protocol::Kitty));
    }

    #[test]
    fn sixel_is_used_when_only_the_da1_reply_lists_it() {
        assert_eq!(drawn_with(&[], Replies { sixel: true, ..Replies::default() }), Some(Protocol::Sixel));
        assert_eq!(drawn_with(&[], Replies::default()), None);
    }

    #[test]
    fn terminals_known_without_graphics_stay_without_sixel() {
        for program in ["iTerm.app", "Apple_Terminal", "vscode"] {
            assert_eq!(drawn_with(&[("TERM_PROGRAM", program)], Replies { sixel: true, ..Replies::default() }), None, "{program}");
        }
    }

    #[test]
    fn the_replied_cell_size_stands_in_for_the_window() {
        let replied = CellSize { width_px: 8, height_px: 16 };
        let replies = Replies { sixel: true, cell: Some(replied), ..Replies::default() };

        assert_eq!(
            graphics(&environment(&[]), ImagesMode::Auto, None, &replies),
            Some(Graphics { protocol: Protocol::Sixel, cell: replied })
        );
        assert_eq!(graphics(&environment(&[]), ImagesMode::Auto, CELL, &replies).map(|graphics| graphics.cell), CELL);
        assert_eq!(graphics(&environment(&[]), ImagesMode::Auto, None, &Replies { cell: None, ..replies }), None);
        assert_eq!(graphics(&environment(&[]), ImagesMode::Never, CELL, &replies), None);
    }

    #[test]
    fn a_named_theme_skips_the_background_question() {
        let asked = questions(&environment(&[("TERM", "xterm-kitty")]), ColorDepth::TrueColor, CELL, Preferences::default());

        assert!(asked.is_empty());
    }

    #[test]
    fn nothing_is_asked_when_stdout_is_not_a_terminal() {
        let preferences = Preferences { needs_background: true, ..Preferences::default() };
        let piped = Environment { stdout_is_tty: false, ..environment(&[]) };

        assert!(questions(&piped, ColorDepth::TrueColor, None, preferences).is_empty());
    }
}
