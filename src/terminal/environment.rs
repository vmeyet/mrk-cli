use std::collections::BTreeMap;
use std::io::IsTerminal;

use super::{ColorChoice, ColorDepth, ImagesMode};
use crate::theme::Appearance;

const VARIABLES: [&str; 14] = [
    "COLORFGBG",
    "COLORTERM",
    "GHOSTTY_RESOURCES_DIR",
    "INSIDE_EMACS",
    "KITTY_WINDOW_ID",
    "LC_TERMINAL",
    "NO_COLOR",
    "STY",
    "TERM",
    "TERM_PROGRAM",
    "TMUX",
    "WEZTERM_EXECUTABLE",
    "WT_SESSION",
    "ZELLIJ",
];
const TRUECOLOR_PROGRAMS: [&str; 4] = ["ghostty", "WezTerm", "iTerm.app", "vscode"];
const TRUECOLOR_TERMS: [&str; 3] = ["xterm-kitty", "xterm-ghostty", "wezterm"];
const GRAPHICS_PROGRAMS: [&str; 2] = ["ghostty", "WezTerm"];
const GRAPHICS_TERMS: [&str; 2] = ["xterm-kitty", "xterm-ghostty"];
/// iTerm2 answers the kitty graphics query with `OK` yet draws none of mrk's pictures.
const NO_GRAPHICS_PROGRAMS: [&str; 3] = ["Apple_Terminal", "iTerm.app", "vscode"];

/// The environment variables detection reads, and whether stdout is a terminal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Environment {
    pub variables: BTreeMap<String, String>,
    pub stdout_is_tty: bool,
}

impl Environment {
    pub fn read() -> Self {
        let variables = VARIABLES.iter().filter_map(|&name| Some((name.to_owned(), std::env::var(name).ok()?))).collect();
        Self { variables, stdout_is_tty: std::io::stdout().is_terminal() }
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.variables.get(name).map(String::as_str)
    }

    fn has(&self, name: &str) -> bool {
        self.get(name).is_some_and(|value| !value.is_empty())
    }

    fn term(&self) -> &str {
        self.get("TERM").unwrap_or_default()
    }

    fn program(&self) -> &str {
        self.get("TERM_PROGRAM").unwrap_or_default()
    }

    fn is_dumb(&self) -> bool {
        self.term() == "dumb"
    }

    fn is_tmux(&self) -> bool {
        self.has("TMUX")
    }

    fn is_multiplexed(&self) -> bool {
        let term = self.term();
        self.is_tmux() || self.has("STY") || self.has("ZELLIJ") || term.starts_with("screen") || term.starts_with("tmux")
    }
}

fn is_truecolor(environment: &Environment) -> bool {
    let colorterm = environment.get("COLORTERM").unwrap_or_default();
    let is_announced = colorterm == "truecolor" || colorterm == "24bit";
    let is_known_program = TRUECOLOR_PROGRAMS.contains(&environment.program());
    let is_known_term = TRUECOLOR_TERMS.contains(&environment.term()) || environment.term().ends_with("-direct");
    let is_known_host = environment.has("KITTY_WINDOW_ID") || environment.has("GHOSTTY_RESOURCES_DIR") || environment.has("WT_SESSION");
    is_announced || is_known_program || is_known_term || is_known_host
}

pub fn color_depth(environment: &Environment, choice: ColorChoice) -> ColorDepth {
    let depth = if is_truecolor(environment) { ColorDepth::TrueColor } else { ColorDepth::Ansi256 };
    let is_off = environment.has("NO_COLOR") || !environment.stdout_is_tty || environment.is_dumb();
    match choice {
        ColorChoice::Never => ColorDepth::None,
        ColorChoice::Auto if is_off => ColorDepth::None,
        ColorChoice::Auto | ColorChoice::Always => depth,
    }
}

pub fn hyperlinks(environment: &Environment, color: ColorDepth) -> bool {
    let is_known_bad = environment.is_dumb() || environment.term() == "linux" || environment.has("INSIDE_EMACS");
    color != ColorDepth::None && !is_known_bad
}

/// Inside tmux, kitty graphics need passthrough and the environment it inherited may name another terminal.
pub fn inside_tmux(environment: &Environment) -> bool {
    environment.is_tmux()
}

/// `Some` when the environment already says whether the terminal draws kitty-protocol pictures; inside tmux it can
/// only say no.
pub fn known_graphics(environment: &Environment) -> Option<bool> {
    let is_known_term = GRAPHICS_TERMS.contains(&environment.term());
    let is_known_program = GRAPHICS_PROGRAMS.contains(&environment.program());
    let is_known_host = environment.has("KITTY_WINDOW_ID") || environment.has("GHOSTTY_RESOURCES_DIR");
    let is_known_with = (is_known_term || is_known_program || is_known_host) && !environment.is_tmux();
    let is_iterm = environment.get("LC_TERMINAL") == Some("iTerm2");
    let is_known_without =
        NO_GRAPHICS_PROGRAMS.contains(&environment.program()) || is_iterm || environment.is_dumb() || environment.term() == "linux";
    match () {
        () if is_known_with => Some(true),
        () if is_known_without => Some(false),
        () => None,
    }
}

pub fn wants_pictures(environment: &Environment, images: ImagesMode) -> bool {
    match images {
        ImagesMode::Never => false,
        ImagesMode::Always => environment.stdout_is_tty,
        ImagesMode::Auto => environment.stdout_is_tty && (environment.is_tmux() || !environment.is_multiplexed()),
    }
}

/// `COLORFGBG` (`"15;0"`, set by rxvt, Konsole and iTerm) names the background by its ANSI index.
pub fn background_hint(environment: &Environment) -> Option<Appearance> {
    let index: u8 = environment.get("COLORFGBG")?.rsplit(';').next()?.parse().ok()?;
    match index {
        0..=6 | 8 => Some(Appearance::Dark),
        7 | 9..=15 => Some(Appearance::Light),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tty(variables: &[(&str, &str)]) -> Environment {
        let variables = variables.iter().map(|&(name, value)| (name.to_owned(), value.to_owned())).collect();
        Environment { variables, stdout_is_tty: true }
    }

    fn piped(variables: &[(&str, &str)]) -> Environment {
        Environment { stdout_is_tty: false, ..tty(variables) }
    }

    #[test]
    fn a_plain_terminal_gets_256_colours() {
        assert_eq!(color_depth(&tty(&[("TERM", "xterm-256color")]), ColorChoice::Auto), ColorDepth::Ansi256);
    }

    #[test]
    fn truecolor_comes_from_colorterm_or_a_known_terminal() {
        for variables in [
            [("COLORTERM", "truecolor")],
            [("COLORTERM", "24bit")],
            [("TERM_PROGRAM", "ghostty")],
            [("TERM_PROGRAM", "WezTerm")],
            [("TERM_PROGRAM", "iTerm.app")],
            [("TERM", "xterm-kitty")],
            [("KITTY_WINDOW_ID", "1")],
            [("GHOSTTY_RESOURCES_DIR", "/Applications/Ghostty.app")],
        ] {
            assert_eq!(color_depth(&tty(&variables), ColorChoice::Auto), ColorDepth::TrueColor, "{variables:?}");
        }
    }

    #[test]
    fn no_color_a_pipe_or_a_dumb_terminal_turn_colour_off() {
        assert_eq!(color_depth(&tty(&[("NO_COLOR", "1")]), ColorChoice::Auto), ColorDepth::None);
        assert_eq!(color_depth(&piped(&[("COLORTERM", "truecolor")]), ColorChoice::Auto), ColorDepth::None);
        assert_eq!(color_depth(&tty(&[("TERM", "dumb")]), ColorChoice::Auto), ColorDepth::None);
    }

    #[test]
    fn an_empty_no_color_is_ignored() {
        assert_eq!(color_depth(&tty(&[("NO_COLOR", "")]), ColorChoice::Auto), ColorDepth::Ansi256);
    }

    #[test]
    fn the_flag_wins_over_the_environment() {
        assert_eq!(color_depth(&piped(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")]), ColorChoice::Always), ColorDepth::TrueColor);
        assert_eq!(color_depth(&tty(&[("COLORTERM", "truecolor")]), ColorChoice::Never), ColorDepth::None);
    }

    #[test]
    fn hyperlinks_follow_colour_except_on_known_bad_terminals() {
        assert!(hyperlinks(&tty(&[]), ColorDepth::Ansi256));
        assert!(!hyperlinks(&tty(&[]), ColorDepth::None));
        assert!(!hyperlinks(&tty(&[("TERM", "linux")]), ColorDepth::Ansi256));
        assert!(!hyperlinks(&tty(&[("INSIDE_EMACS", "29.1,vterm")]), ColorDepth::TrueColor));
    }

    #[test]
    fn graphics_are_known_from_the_environment() {
        for variables in [
            [("TERM", "xterm-kitty")],
            [("TERM", "xterm-ghostty")],
            [("TERM_PROGRAM", "ghostty")],
            [("TERM_PROGRAM", "WezTerm")],
            [("KITTY_WINDOW_ID", "3")],
            [("GHOSTTY_RESOURCES_DIR", "/x")],
        ] {
            assert_eq!(known_graphics(&tty(&variables)), Some(true), "{variables:?}");
        }
        assert_eq!(known_graphics(&tty(&[("TERM_PROGRAM", "Apple_Terminal")])), Some(false));
        assert_eq!(known_graphics(&tty(&[("TERM_PROGRAM", "iTerm.app")])), Some(false));
        assert_eq!(known_graphics(&tty(&[("TERM", "xterm-256color")])), None);
    }

    #[test]
    fn inside_tmux_the_inherited_terminal_is_not_trusted_but_iterm_still_is_ruled_out() {
        assert_eq!(known_graphics(&tty(&[("TMUX", "/tmp/tmux"), ("GHOSTTY_RESOURCES_DIR", "/x"), ("TERM", "tmux-256color")])), None);
        assert_eq!(known_graphics(&tty(&[("TMUX", "/tmp/tmux"), ("LC_TERMINAL", "iTerm2")])), Some(false));
        assert!(inside_tmux(&tty(&[("TMUX", "/tmp/tmux")])));
        assert!(!inside_tmux(&tty(&[("TERM", "tmux-256color")])));
    }

    #[test]
    fn pictures_need_a_terminal_and_no_multiplexer_but_tmux_unless_forced() {
        assert!(wants_pictures(&tty(&[]), ImagesMode::Auto));
        assert!(wants_pictures(&tty(&[("TMUX", "/tmp/tmux")]), ImagesMode::Auto));
        assert!(!wants_pictures(&piped(&[]), ImagesMode::Auto));
        assert!(!wants_pictures(&piped(&[]), ImagesMode::Always));
        assert!(!wants_pictures(&tty(&[]), ImagesMode::Never));
        for variables in [[("ZELLIJ", "0")], [("STY", "1.pts")], [("TERM", "screen-256color")], [("TERM", "tmux-256color")]] {
            assert!(!wants_pictures(&tty(&variables), ImagesMode::Auto), "{variables:?}");
            assert!(wants_pictures(&tty(&variables), ImagesMode::Always), "{variables:?}");
        }
    }

    #[test]
    fn colorfgbg_names_the_background() {
        assert_eq!(background_hint(&tty(&[("COLORFGBG", "15;0")])), Some(Appearance::Dark));
        assert_eq!(background_hint(&tty(&[("COLORFGBG", "0;15")])), Some(Appearance::Light));
        assert_eq!(background_hint(&tty(&[("COLORFGBG", "0;default;7")])), Some(Appearance::Light));
        assert_eq!(background_hint(&tty(&[("COLORFGBG", "0;default")])), None);
        assert_eq!(background_hint(&tty(&[])), None);
    }
}
