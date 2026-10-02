//! The command line: flags win over the environment (`MRK_THEME`, `MRK_WIDTH`, `MRK_ALIGN`), the environment over the config file.
mod help;
mod input;
mod output;

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

use crate::config::{self, Config, MIN_WIDTH};
use crate::document::{Block, Document, Line, Rgb, Span, Style};
use crate::terminal::{self, Align, ColorChoice, ColorDepth, ImagesMode, Preferences};
use crate::theme::{self, Appearance, Palette, Theme};
use output::{Layout, Output};

const SWATCH: &str = "██";
const TERMINAL_MARK: &str = " (your terminal)";

/// Render Markdown beautifully in the terminal.
#[derive(Parser, Debug)]
#[command(name = "mrk", version = crate::version::label(), about, after_long_help = help::text(), args_conflicts_with_subcommands = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    /// The Markdown file to render; stdin when omitted or `-`.
    pub file: Option<PathBuf>,
    /// The theme, one of `--list-themes`; it wins over the pair `theme_dark`/`theme_light` from the config, picked after
    /// the terminal background like the default `mrk-dark`/`mrk-light`.
    #[arg(long, env = "MRK_THEME", value_name = "NAME")]
    pub theme: Option<String>,
    /// List the themes with a swatch of their colours.
    #[arg(long, conflicts_with = "file")]
    pub list_themes: bool,
    /// Wrap width in columns; by default the terminal width, capped at 100.
    #[arg(long, env = "MRK_WIDTH", value_name = "N", value_parser = parse_width)]
    pub width: Option<usize>,
    /// Draw Mermaid diagrams as images, on a terminal that speaks the kitty graphics protocol or Sixel.
    ///
    /// `auto` draws them outside screen and zellij, and inside tmux once `set -g allow-passthrough on`;
    /// `always` tries inside screen and zellij too, `never` draws diagrams as text.
    /// Piped output never gets images.
    #[arg(long, value_enum, value_name = "WHEN")]
    pub images: Option<ImagesMode>,
    /// Place the text in the window; `center` only applies on a terminal, never to piped output.
    #[arg(long, value_enum, env = "MRK_ALIGN", value_name = "WHERE")]
    pub align: Option<Align>,
    /// Read in the built-in pager, which keeps diagrams as images.
    ///
    /// `MRK_PAGER` swaps it for that command (diagrams as text), but only this flag or `pager = true` turns paging on.
    /// Piped output is never paged.
    #[arg(short, long)]
    pub pager: bool,
    /// Draw level-1 headings two rows tall.
    ///
    /// As a picture over the concealed title on a terminal that speaks the kitty graphics protocol, outside tmux;
    /// as double-height text on xterm, Konsole, Windows Terminal, mlterm and iTerm2; as a normal heading elsewhere.
    #[arg(long)]
    pub jumbo_title: bool,
    /// Colour the output; `auto` honours NO_COLOR and a stdout that is not a terminal.
    #[arg(long, value_enum, value_name = "WHEN", default_value_t)]
    pub color: ColorChoice,
    /// Print the completion script for a shell.
    #[arg(long, value_name = "SHELL", exclusive = true)]
    pub completions: Option<Shell>,
    /// Print the man page as roff.
    #[arg(long, hide = true, exclusive = true)]
    pub man: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Install the latest mrk release, through brew when brew installed mrk, else with cargo; a file named `update` renders as `./update`.
    Update {
        /// Install even when the running binary is already the latest release.
        #[arg(short, long)]
        force: bool,
    },
}

/// A mistake in how mrk was called, by flag, environment or config file, rather than a failure while running: exits with 2.
#[derive(Debug)]
pub struct UsageError(pub String);

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

fn parse_width(raw: &str) -> Result<usize, String> {
    let width: usize = raw.parse().map_err(|_| format!("`{raw}` is not a number of columns"))?;
    if width < MIN_WIDTH { Err(format!("{width} is too narrow: {MIN_WIDTH} is the least")) } else { Ok(width) }
}

fn print_completions(shell: Shell) {
    clap_complete::generate(shell, &mut Cli::command(), "mrk", &mut io::stdout());
}

fn print_man() -> io::Result<()> {
    let man = clap_mangen::Man::new(Cli::command());
    let out = &mut io::stdout().lock();
    man.render_title(out)?;
    man.render_name_section(out)?;
    man.render_synopsis_section(out)?;
    man.render_description_section(out)?;
    man.render_options_section(out)?;
    man.render_subcommands_section(out)?;
    out.write_all(help::roff().as_bytes())?;
    man.render_version_section(out)
}

fn theme_choice(cli: &Cli, config: &Config) -> Result<theme::Choice> {
    let name = cli.theme.as_deref().or(config.theme.as_deref());
    theme::Choice::new(name, config.theme_dark.as_deref(), config.theme_light.as_deref())
        .map_err(|error| UsageError(error.to_string()).into())
}

fn palette_colors(palette: &Palette) -> [Rgb; 16] {
    let Palette { text, muted, subtle, surface, accent, h1, h2, h3, link, code, success, note, tip, important, warning, caution } =
        *palette;
    [text, muted, subtle, surface, accent, h1, h2, h3, link, code, success, note, tip, important, warning, caution]
}

fn theme_line(theme: &Theme, name_width: usize) -> Line {
    let name = Span::new(format!("{:name_width$}  ", theme.name), Style::fg(theme.palette.text).bold());
    let swatches = palette_colors(&theme.palette).into_iter().map(|color| Span::new(SWATCH, Style::fg(color)));
    Line::new(std::iter::once(name).chain(swatches).collect())
}

fn group_heading(appearance: Appearance, background: Option<Appearance>) -> Line {
    let title = match appearance {
        Appearance::Dark => "Dark",
        Appearance::Light => "Light",
    };
    let mark = if background == Some(appearance) { TERMINAL_MARK } else { "" };
    Line::new(vec![Span::new(title, Style::default().bold()), Span::plain(mark)])
}

fn theme_group(themes: &[Theme], appearance: Appearance, background: Option<Appearance>, name_width: usize) -> Vec<Line> {
    let lines = themes.iter().filter(|theme| theme.appearance == appearance).map(|theme| theme_line(theme, name_width));
    std::iter::once(group_heading(appearance, background)).chain(lines).collect()
}

/// Coloured, the themes come in a dark and a light group, the one matching `background` marked; else bare names for scripts.
fn theme_list(has_color: bool, background: Option<Appearance>) -> Document {
    let themes: Vec<Theme> = theme::names().filter_map(theme::find).collect();
    if !has_color {
        return Document { blocks: vec![Block::Lines(themes.iter().map(|theme| Line::new(vec![Span::plain(theme.name)])).collect())] };
    }
    let name_width = themes.iter().map(|theme| theme.name.len()).max().unwrap_or_default();
    let groups = [Appearance::Dark, Appearance::Light].map(|appearance| theme_group(&themes, appearance, background, name_width));
    Document { blocks: vec![Block::Lines(groups.join(&Line::blank()))] }
}

fn list_themes(color: ColorChoice) -> Result<()> {
    let capabilities = terminal::detect(Preferences { color, images: ImagesMode::Never, needs_background: true });
    let margin = if capabilities.is_terminal { terminal::LEFT_MARGIN } else { 0 };
    output::print(&theme_list(capabilities.color != ColorDepth::None, capabilities.background), &capabilities, margin)
}

fn render(cli: &Cli, config: &Config) -> Result<()> {
    let theme = theme_choice(cli, config)?;
    let source = input::read_input(cli.file.as_deref())?;
    let wants_pager = cli.pager || config.pager.unwrap_or_default();
    let output = output::choose(wants_pager, io::stdout().is_terminal(), std::env::var("MRK_PAGER").ok().as_deref())?;
    let wanted_images = cli.images.or(config.images).unwrap_or_default();
    let is_command = matches!(output, Output::Command(_));
    let images = if is_command { ImagesMode::Never } else { wanted_images };
    let wants_jumbo_title = cli.jumbo_title || config.jumbo_title.unwrap_or_default();
    let capabilities = terminal::detect(Preferences { color: cli.color, images, needs_background: theme.needs_background() });

    let layout = Layout {
        source,
        requested_width: cli.width.or(config.width),
        align: cli.align.or(config.align).unwrap_or_default(),
        theme: theme.resolve(capabilities.background),
        jumbo_title: wants_jumbo_title && !is_command,
        capabilities,
    };
    output::show(&output, &layout, &input::name(cli.file.as_deref()))
}

/// clap echoes arguments and environment values verbatim, so its errors are sanitized; help and version carry nothing
/// from the user and keep their colours.
fn exit_on(error: &clap::Error) -> ! {
    if !error.use_stderr() {
        error.exit();
    }
    eprint!("{}", terminal::sanitize::lines(&error.render().to_string()));
    std::process::exit(error.exit_code())
}

pub fn run() -> Result<()> {
    let cli = Cli::try_parse().unwrap_or_else(|error| exit_on(&error));
    if let Some(Command::Update { force }) = cli.command {
        return crate::update::run(force);
    }
    if let Some(shell) = cli.completions {
        print_completions(shell);
        return Ok(());
    }
    if cli.man {
        return output::ignore_broken_pipe(print_man()).context("writing the man page");
    }

    let config = config::load()?;
    if cli.list_themes {
        return list_themes(cli.color);
    }
    render(&cli, &config)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn update_is_a_command_and_a_path_still_renders() {
        assert!(matches!(parse(&["update", "--force"]).command, Some(Command::Update { force: true })));
        assert_eq!(parse(&["./update"]).file.as_deref(), Some(std::path::Path::new("./update")));
    }

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("mrk").chain(args.iter().copied())).unwrap()
    }

    #[test]
    fn every_flag_of_the_usage_parses() {
        let cli = parse(&["--theme", "mrk-dark", "--width", "72", "--images", "never", "--color", "always", "--jumbo-title", "notes.md"]);

        assert_eq!(cli.file, Some(PathBuf::from("notes.md")));
        assert_eq!(cli.theme.as_deref(), Some("mrk-dark"));
        assert_eq!(cli.width, Some(72));
        assert_eq!(cli.images, Some(ImagesMode::Never));
        assert_eq!(cli.color, ColorChoice::Always);
        assert!(cli.jumbo_title);
    }

    #[test]
    fn colour_defaults_to_auto() {
        assert_eq!(parse(&[]).color, ColorChoice::Auto);
    }

    #[test]
    fn a_narrow_or_bad_width_is_refused() {
        assert!(Cli::try_parse_from(["mrk", "--width", "19"]).is_err());
        assert!(Cli::try_parse_from(["mrk", "--width", "wide"]).is_err());
    }

    #[test]
    fn completions_stand_alone() {
        assert!(Cli::try_parse_from(["mrk", "--completions", "zsh", "a.md"]).is_err());
    }

    fn config_with_pair(dark: &str, light: &str) -> Config {
        Config { theme_dark: Some(dark.to_owned()), theme_light: Some(light.to_owned()), ..Config::default() }
    }

    #[test]
    fn an_unknown_theme_is_a_usage_error_listing_the_known_ones() {
        let error = theme_choice(&parse(&["--theme", "nope"]), &Config::default()).unwrap_err();
        let message = error.to_string();

        assert!(error.is::<UsageError>());
        assert!(message.contains("\"nope\"") && message.contains("mrk-dark"), "{message}");
    }

    #[test]
    fn an_unknown_theme_in_the_config_pair_is_a_usage_error() {
        let error = theme_choice(&parse(&[]), &config_with_pair("nord", "nope")).unwrap_err();

        assert!(error.is::<UsageError>());
        assert!(error.to_string().contains("\"nope\""), "{error}");
    }

    #[test]
    fn the_theme_flag_wins_over_the_config_pair() {
        let choice = theme_choice(&parse(&["--theme", "dracula"]), &config_with_pair("nord", "github-light")).unwrap();

        assert_eq!(choice.resolve(Some(Appearance::Light)).name, "dracula");
    }

    #[test]
    fn the_config_pair_follows_the_background() {
        let choice = theme_choice(&parse(&[]), &config_with_pair("nord", "github-light")).unwrap();

        assert!(choice.needs_background());
        assert_eq!(choice.resolve(Some(Appearance::Light)).name, "github-light");
    }

    #[test]
    fn a_hostile_theme_name_is_escaped_in_the_error() {
        let error = theme_choice(&parse(&[]), &config_with_pair("\x1b]52;c;AAAA\x07", "mrk-light")).unwrap_err();

        assert!(!error.to_string().contains('\x1b'));
    }

    #[test]
    fn the_theme_list_shows_every_palette_colour_when_coloured() {
        let listed = theme_list(true, None);
        let Block::Lines(lines) = &listed.blocks[0] else { panic!("a theme list is lines") };

        assert_eq!(lines[1].spans.len(), 17);
        assert!(lines[1].plain().starts_with("mrk-dark ") && lines[1].plain().ends_with("██"));
    }

    #[test]
    fn the_theme_list_groups_dark_then_light_and_marks_the_terminal_background() {
        insta::assert_snapshot!(crate::document::plain(&theme_list(true, Some(Appearance::Light))));
    }

    #[test]
    fn the_theme_list_marks_no_group_when_the_background_is_unknown() {
        assert!(!crate::document::plain(&theme_list(true, None)).contains(TERMINAL_MARK));
    }

    #[test]
    fn the_theme_list_is_names_only_without_colour() {
        let expected: String = theme::names().map(|name| format!("{name}\n")).collect();

        assert_eq!(crate::document::plain(&theme_list(false, Some(Appearance::Dark))), expected);
    }
}
