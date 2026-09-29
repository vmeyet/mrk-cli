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

/// Render Markdown beautifully in the terminal.
#[derive(Parser, Debug)]
#[command(name = "mrk", version = crate::version::label(), about, after_long_help = help::text(), args_conflicts_with_subcommands = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    /// The Markdown file to render; stdin when omitted or `-`.
    pub file: Option<PathBuf>,
    /// The theme, one of `--list-themes`; by default dark or light after the terminal background.
    #[arg(long, env = "MRK_THEME", value_name = "NAME")]
    pub theme: Option<String>,
    /// List the themes with a swatch of their colours.
    #[arg(long, conflicts_with = "file")]
    pub list_themes: bool,
    /// Wrap width in columns; by default the terminal width, capped at 100.
    #[arg(long, env = "MRK_WIDTH", value_name = "N", value_parser = parse_width)]
    pub width: Option<usize>,
    /// Draw Mermaid diagrams as images, on a terminal that speaks the kitty graphics protocol.
    ///
    /// `auto` draws them outside tmux and screen, `always` inside them too, `never` draws diagrams as text.
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
    /// Rebuild and install the latest mrk release with cargo; a file named `update` renders as `./update`.
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

fn find_theme(name: &str) -> Result<Theme> {
    theme::resolve(Some(name), None).map_err(|error| UsageError(error.to_string()).into())
}

fn default_theme(background: Option<Appearance>) -> Theme {
    theme::default_for(background.unwrap_or(Appearance::Dark))
}

fn palette_colors(palette: &Palette) -> [Rgb; 16] {
    let Palette { text, muted, subtle, surface, accent, h1, h2, h3, link, code, success, note, tip, important, warning, caution } =
        *palette;
    [text, muted, subtle, surface, accent, h1, h2, h3, link, code, success, note, tip, important, warning, caution]
}

fn theme_line(theme: &Theme, name_width: usize, has_color: bool) -> Line {
    if !has_color {
        return Line::new(vec![Span::plain(theme.name)]);
    }
    let name = Span::new(format!("{:name_width$}  ", theme.name), Style::fg(theme.palette.text).bold());
    let swatches = palette_colors(&theme.palette).into_iter().map(|color| Span::new(SWATCH, Style::fg(color)));
    Line::new(std::iter::once(name).chain(swatches).collect())
}

fn theme_list(has_color: bool) -> Document {
    let themes: Vec<Theme> = theme::names().filter_map(theme::find).collect();
    let name_width = themes.iter().map(|theme| theme.name.len()).max().unwrap_or_default();
    Document { blocks: vec![Block::Lines(themes.iter().map(|theme| theme_line(theme, name_width, has_color)).collect())] }
}

fn list_themes(color: ColorChoice) -> Result<()> {
    let capabilities = terminal::detect(Preferences { color, images: ImagesMode::Never, needs_background: false });
    let margin = if capabilities.is_terminal { terminal::LEFT_MARGIN } else { 0 };
    output::print(&theme_list(capabilities.color != ColorDepth::None), &capabilities, margin)
}

fn render(cli: &Cli, config: &Config) -> Result<()> {
    let theme = cli.theme.as_deref().or(config.theme.as_deref()).map(find_theme).transpose()?;
    let source = input::read_input(cli.file.as_deref())?;
    let wants_pager = cli.pager || config.pager.unwrap_or_default();
    let output = output::choose(wants_pager, io::stdout().is_terminal(), std::env::var("MRK_PAGER").ok().as_deref())?;
    let wanted_images = cli.images.or(config.images).unwrap_or_default();
    let images = if matches!(output, Output::Command(_)) { ImagesMode::Never } else { wanted_images };
    let capabilities = terminal::detect(Preferences { color: cli.color, images, needs_background: theme.is_none() });

    let layout = Layout {
        source,
        requested_width: cli.width.or(config.width),
        align: cli.align.or(config.align).unwrap_or_default(),
        theme: theme.unwrap_or_else(|| default_theme(capabilities.background)),
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
        let cli = parse(&["--theme", "mrk-dark", "--width", "72", "--images", "never", "--color", "always", "notes.md"]);

        assert_eq!(cli.file, Some(PathBuf::from("notes.md")));
        assert_eq!(cli.theme.as_deref(), Some("mrk-dark"));
        assert_eq!(cli.width, Some(72));
        assert_eq!(cli.images, Some(ImagesMode::Never));
        assert_eq!(cli.color, ColorChoice::Always);
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

    #[test]
    fn an_unknown_theme_is_a_usage_error_listing_the_known_ones() {
        let error = find_theme("nope").unwrap_err();
        let message = error.to_string();

        assert!(error.is::<UsageError>());
        assert!(message.contains("\"nope\"") && message.contains("mrk-dark"), "{message}");
    }

    #[test]
    fn a_hostile_theme_name_is_escaped_in_the_error() {
        assert!(!find_theme("\x1b]52;c;AAAA\x07").unwrap_err().to_string().contains('\x1b'));
    }

    #[test]
    fn the_theme_list_shows_every_palette_colour_when_coloured() {
        let listed = theme_list(true);
        let Block::Lines(lines) = &listed.blocks[0] else { panic!("a theme list is lines") };

        assert_eq!(lines[0].spans.len(), 17);
        assert!(lines[0].plain().starts_with("mrk-dark ") && lines[0].plain().ends_with("██"));
        assert_eq!(lines.len(), theme::names().count());
    }

    #[test]
    fn the_theme_list_is_names_only_without_colour() {
        let expected: String = theme::names().map(|name| format!("{name}\n")).collect();

        assert_eq!(crate::document::plain(&theme_list(false)), expected);
    }
}
