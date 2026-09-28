//! The command line: flags win over the environment (`MRK_THEME`, `MRK_WIDTH`), the environment over the config file.
use std::fmt;
use std::io::{self, BufWriter, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser};
use clap_complete::Shell;

use crate::config::{self, Config, MIN_WIDTH};
use crate::document::{Block, Document, Line, Rgb, Settings, Span, Style};
use crate::terminal::{self, Capabilities, ColorChoice, ColorDepth, ImagesMode, Preferences};
use crate::theme::{self, Appearance, Palette, Theme};

const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const SIDE_MARGINS: usize = 4;
const SWATCH: &str = "██";
const USAGE_HINT: &str = "no input: name a Markdown file (mrk README.md) or pipe one in (cat notes.md | mrk)";

/// Render Markdown beautifully in the terminal.
#[derive(Parser, Debug)]
#[command(name = "mrk", version, about)]
pub struct Cli {
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
    /// Draw Mermaid diagrams as images; `auto` does when the terminal supports it.
    #[arg(long, value_enum, value_name = "WHEN")]
    pub images: Option<ImagesMode>,
    /// Colour the output; `auto` honours NO_COLOR and a stdout that is not a terminal.
    #[arg(long, value_enum, value_name = "WHEN", default_value_t)]
    pub color: ColorChoice,
    /// Print the completion script for a shell.
    #[arg(long, value_name = "SHELL", exclusive = true)]
    pub completions: Option<Shell>,
}

/// A mistake in how mrk was called rather than a failure while running: exits with 2.
#[derive(Debug)]
pub struct UsageError(pub &'static str);

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
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

fn find_theme(name: &str) -> Result<Theme> {
    theme::resolve(Some(name), None)
}

fn default_theme(background: Option<Appearance>) -> Theme {
    theme::default_for(background.unwrap_or(Appearance::Dark))
}

fn default_width(columns: u16) -> usize {
    usize::from(columns).saturating_sub(SIDE_MARGINS).min(theme::DEFAULT_WIDTH)
}

fn width(requested: Option<usize>, columns: u16) -> usize {
    requested.unwrap_or_else(|| default_width(columns)).max(MIN_WIDTH)
}

fn read_capped(reader: impl Read, name: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(MAX_INPUT_BYTES as u64 + 1).read_to_end(&mut bytes).with_context(|| format!("reading {name}"))?;
    if bytes.len() > MAX_INPUT_BYTES {
        bail!("{name} is larger than 8 MiB, the most mrk reads");
    }
    Ok(bytes)
}

/// Invalid UTF-8 is replaced rather than refused: a stray Latin-1 byte should not hide an otherwise readable file,
/// and U+FFFD is inert on a terminal.
fn decode(bytes: Vec<u8>, name: &str) -> String {
    String::from_utf8(bytes).unwrap_or_else(|error| {
        eprintln!("⚠ {name} is not valid UTF-8: invalid bytes show as �");
        String::from_utf8_lossy(error.as_bytes()).into_owned()
    })
}

fn read_file(path: &Path) -> Result<String> {
    let name = path.display().to_string();
    let file = std::fs::File::open(path).with_context(|| format!("reading {name}"))?;
    Ok(decode(read_capped(file, &name)?, &name))
}

fn read_stdin() -> Result<String> {
    Ok(decode(read_capped(io::stdin().lock(), "stdin")?, "stdin"))
}

fn read_input(file: Option<&Path>) -> Result<String> {
    match file {
        None if io::stdin().is_terminal() => Err(UsageError(USAGE_HINT).into()),
        Some(path) if path != Path::new("-") => read_file(path),
        _ => read_stdin(),
    }
}

fn print(document: &Document, capabilities: &Capabilities) -> Result<()> {
    let mut out = BufWriter::new(io::stdout().lock());
    let written = terminal::write(document, capabilities, &mut out).and_then(|()| out.flush());
    match written {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other.context("writing to stdout"),
    }
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
    print(&theme_list(capabilities.color != ColorDepth::None), &capabilities)
}

fn render(cli: &Cli, config: &Config) -> Result<()> {
    let theme = cli.theme.as_deref().or(config.theme.as_deref()).map(find_theme).transpose()?;
    let source = read_input(cli.file.as_deref())?;
    let images = cli.images.or(config.images).unwrap_or_default();
    let capabilities = terminal::detect(Preferences { color: cli.color, images, needs_background: theme.is_none() });

    let settings = Settings {
        width: width(cli.width.or(config.width), capabilities.columns),
        theme: theme.unwrap_or_else(|| default_theme(capabilities.background)),
        cell: capabilities.cell,
    };
    print(&crate::markdown::render(&source, &settings), &capabilities)
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Some(shell) = cli.completions {
        print_completions(shell);
        return Ok(());
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
    fn the_default_width_leaves_margins_and_caps_at_100() {
        assert_eq!(width(None, 80), 76);
        assert_eq!(width(None, 200), 100);
        assert_eq!(width(None, 10), MIN_WIDTH);
        assert_eq!(width(Some(120), 80), 120);
    }

    #[test]
    fn an_unknown_theme_lists_the_known_ones() {
        let error = find_theme("nope").unwrap_err().to_string();

        assert!(error.contains("\"nope\"") && error.contains("mrk-dark"), "{error}");
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

    #[test]
    fn oversized_input_is_refused() {
        let error = read_capped(io::repeat(b'a').take(MAX_INPUT_BYTES as u64 + 1), "big.md").unwrap_err().to_string();

        assert!(error.contains("big.md") && error.contains("8 MiB"), "{error}");
        assert_eq!(read_capped(io::repeat(b'a').take(MAX_INPUT_BYTES as u64), "ok.md").unwrap().len(), MAX_INPUT_BYTES);
    }

    #[test]
    fn invalid_utf8_is_replaced() {
        assert_eq!(decode(b"caf\xe9".to_vec(), "x.md"), "caf\u{fffd}");
    }
}
