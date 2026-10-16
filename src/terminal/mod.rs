mod ansi;
mod color;
mod detect;
mod environment;
mod kitty;
pub mod pager;
mod query;
mod reply;
mod sixel;
mod tmux;

use std::io::Write;

use serde::Deserialize;

pub use detect::{Preferences, detect};

use self::ansi::Half;
use self::kitty::Crop;
use self::pager::picture;
use self::sixel::Image;
use crate::document::{Block, CellSize, Document, JumboTitle, Picture};
use crate::sanitize;
use crate::theme::Appearance;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    None,
    Ansi256,
    TrueColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub color: ColorDepth,
    pub hyperlinks: bool,
    /// Set when the terminal draws pictures and pictures are wanted.
    pub graphics: Option<Graphics>,
    /// Whether the terminal draws DEC double-height lines.
    pub double_height: bool,
    pub background: Option<Appearance>,
    pub columns: u16,
    pub rows: u16,
    pub is_terminal: bool,
}

/// How the terminal draws pictures; kitty when it speaks both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Kitty,
    /// Kitty graphics from inside tmux: passed through it and shown by Unicode placeholder cells.
    KittyThroughTmux,
    Sixel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Graphics {
    pub protocol: Protocol,
    pub cell: CellSize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImagesMode {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    #[default]
    Center,
}

pub const LEFT_MARGIN: usize = 2;

/// Columns left of the text: centred in the window when asked and stdout is a terminal, the fixed margin otherwise.
pub fn margin(align: Align, capabilities: &Capabilities, width: usize) -> usize {
    let centred = usize::from(capabilities.columns).saturating_sub(width) / 2;
    let is_centred = align == Align::Center && capabilities.is_terminal;
    if is_centred { centred.max(LEFT_MARGIN) } else { LEFT_MARGIN }
}

/// How this terminal draws a jumbo title: a picture where kitty graphics reach it straight, double-height text where
/// it draws that, `None` otherwise.
pub fn jumbo_title(capabilities: &Capabilities) -> Option<JumboTitle> {
    match capabilities.graphics.map(|graphics| graphics.protocol) {
        Some(Protocol::Kitty) => Some(JumboTitle::Picture),
        _ if capabilities.double_height => Some(JumboTitle::DoubleHeight),
        _ => None,
    }
}

/// What stands left of each row of a picture: the margin, then its indent.
fn picture_indent(picture: &Picture, capabilities: &Capabilities, margin: usize) -> String {
    format!("{}{}", " ".repeat(margin), ansi::line(&picture.indent, capabilities, 0))
}

/// What stands in for a picture, after `indent`, when the terminal cannot draw it.
fn placeholder(picture: &Picture, indent: &str) -> String {
    format!("{indent}[picture: {}]\n", sanitize::text(&picture.alt))
}

/// Rows of a picture drawn together, and the part of its PNG behind them; `None` when they are the whole picture.
struct Run {
    rows: u16,
    crop: Option<Crop>,
}

/// The picture's rows in runs one row shorter than the screen, so the cursor can climb back to a run's first row
/// once its rows scrolled into view: a single run when the picture fits. `None` when a cut is needed and the PNG
/// cannot be read.
fn runs(picture: &Picture, screen_rows: u16) -> Option<Vec<Run>> {
    let rows = picture.rows.max(1);
    let longest = screen_rows.saturating_sub(1).max(1);
    if rows <= longest {
        return Some(vec![Run { rows, crop: None }]);
    }
    let size = picture::png_size(&picture.png)?;
    let run = |start: u16| {
        let end = rows.min(start + longest);
        Run { rows: end - start, crop: Some(picture::crop(start..end, rows, size)) }
    };
    Some((0..rows).step_by(usize::from(longest)).map(run).collect())
}

/// Run after run, the rows written first, after `indent` on each and the concealed text on the picture's first,
/// scrolling them into view; then `draw` of the run at its first row, which leaves the cursor where it found it; then
/// the cursor at the start of the line below. Drawing at the bottom of the screen must not clip the picture.
fn over_rows(picture: &Picture, indent: &str, screen_rows: u16, draw: impl Fn(&Run) -> String) -> Option<String> {
    let drawn = |(index, run): (usize, &Run)| {
        let concealed = if index == 0 { ansi::concealed(picture) } else { String::new() };
        let rest = format!("{indent}\n").repeat(usize::from(run.rows - 1));
        format!("{indent}{concealed}\n{rest}\x1b[{0}A\r{indent}{1}\x1b[{0}B\r", run.rows, draw(run))
    };
    Some(runs(picture, screen_rows)?.iter().enumerate().map(drawn).collect())
}

fn picture(picture: &Picture, capabilities: &Capabilities, margin: usize) -> String {
    let indent = picture_indent(picture, capabilities, margin);
    let screen_rows = capabilities.rows;
    let drawn = capabilities.graphics.and_then(|graphics| match graphics.protocol {
        Protocol::Kitty => over_rows(picture, &indent, screen_rows, |run| kitty::draw(picture, run.rows, run.crop)),
        Protocol::KittyThroughTmux => kitty::draw_through_tmux(picture, &indent),
        Protocol::Sixel => {
            let image = Image::from_png(&picture.png)?;
            over_rows(picture, &indent, screen_rows, |run| sixel::draw(&image, run.crop.map(Crop::pixel_rows)))
        }
    });
    drawn.unwrap_or_else(|| placeholder(picture, &indent))
}

fn block(block: &Block, capabilities: &Capabilities, margin: usize) -> String {
    match block {
        Block::Lines(lines) => lines.iter().map(|line| ansi::line(line, capabilities, margin) + "\n").collect(),
        Block::Picture(each) => picture(each, capabilities, margin),
        Block::DoubleHeight(lines) => lines
            .iter()
            .flat_map(|line| [Half::Top, Half::Bottom].map(|half| ansi::half(line, half, capabilities, margin) + "\n"))
            .collect(),
    }
}

/// Writes the document for this terminal, `margin` columns from the left.
pub fn write(document: &Document, capabilities: &Capabilities, margin: usize, out: &mut impl Write) -> std::io::Result<()> {
    document.blocks.iter().try_for_each(|each| out.write_all(block(each, capabilities, margin).as_bytes()))
}

fn cleaned(message: &dyn std::fmt::Display) -> String {
    sanitize::lines(message.to_string().trim_end())
}

fn indented(message: &str, (dim, reset): (&str, &str)) -> String {
    message.lines().map(|line| format!("  {dim}{line}{reset}\n")).collect()
}

/// An error and its causes for stderr, sanitized, every line after the first indented, causes dimmed when `is_styled`.
pub fn error_report(error: &anyhow::Error, is_styled: bool) -> String {
    let dimmed = if is_styled { ("\x1b[2m", "\x1b[0m") } else { ("", "") };
    let causes: String = error.chain().skip(1).map(|cause| indented(&cleaned(cause), dimmed)).collect();
    format!("✗ {}\n{causes}", cleaned(error).replace('\n', "\n  "))
}

/// The bytes `write` produces, as a string.
#[cfg(test)]
pub fn ansi(document: &Document, capabilities: &Capabilities, margin: usize) -> String {
    document.blocks.iter().map(|each| block(each, capabilities, margin)).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn centring_splits_the_spare_columns_on_a_terminal_only() {
        let wide = Capabilities { columns: 200, ..capabilities(ColorDepth::TrueColor, None) };
        let piped = Capabilities { is_terminal: false, ..wide };
        let narrow = Capabilities { columns: 82, ..wide };

        assert_eq!(margin(Align::Center, &wide, 100), 50);
        assert_eq!(margin(Align::Left, &wide, 100), LEFT_MARGIN);
        assert_eq!(margin(Align::Center, &piped, 100), LEFT_MARGIN);
        assert_eq!(margin(Align::Center, &narrow, 80), LEFT_MARGIN);
    }

    #[test]
    fn an_error_report_strips_escapes_and_dims_causes_only_when_styled() {
        let error = anyhow::anyhow!("inner \x1b]52;c;AAAA\x07").context("cannot read \x1b[31mfile");

        assert_eq!(error_report(&error, false), "✗ cannot read [31mfile\n  inner ]52;c;AAAA\n");
        assert_eq!(error_report(&error, true), "✗ cannot read [31mfile\n  \x1b[2minner ]52;c;AAAA\x1b[0m\n");
    }

    #[test]
    fn an_error_report_keeps_every_line_of_a_message_indented() {
        let error = anyhow::anyhow!("fatal: no remote\r\nhint: add one\n").context("update failed\nfor the tag");

        assert_eq!(
            error_report(&error, true),
            "✗ update failed\n  for the tag\n  \x1b[2mfatal: no remote\x1b[0m\n  \x1b[2mhint: add one\x1b[0m\n"
        );
    }

    use crate::document::{Line, Span, Style};
    use crate::theme::MRK_DARK;

    fn sample() -> Document {
        let palette = MRK_DARK.palette;
        let heading = Line::new(vec![Span::new("Title", Style::fg(palette.h1).bold())]);
        let prose = Line::new(vec![
            Span::new("Read ", Style::fg(palette.text)),
            Span::new("the docs", Style::fg(palette.link).underline()).linked("https://example.com/docs"),
            Span::new(" or run ", Style::fg(palette.text)),
            Span::new(" mrk ", Style::fg(palette.code).on(palette.surface)),
            Span::new(".", Style::fg(palette.text)),
        ]);
        let picture = Picture {
            png: vec![0x89, b'P', b'N', b'G'],
            cols: 20,
            rows: 2,
            alt: "flow".to_owned(),
            indent: Line::blank(),
            concealed_text: None,
        };
        Document { blocks: vec![Block::Lines(vec![heading, Line::blank(), prose]), Block::Picture(picture)] }
    }

    const CELL: CellSize = CellSize { width_px: 10, height_px: 20 };
    const KITTY: Option<Graphics> = Some(Graphics { protocol: Protocol::Kitty, cell: CELL });

    fn capabilities(color: ColorDepth, graphics: Option<Graphics>) -> Capabilities {
        Capabilities {
            color,
            hyperlinks: color != ColorDepth::None,
            graphics,
            double_height: false,
            background: None,
            columns: 80,
            rows: 24,
            is_terminal: true,
        }
    }

    fn readable(ansi: &str) -> String {
        ansi.replace('\x1b', "␛").replace('\r', "␍")
    }

    #[test]
    fn truecolor_with_links_and_pictures() {
        insta::assert_snapshot!(readable(&ansi(&sample(), &capabilities(ColorDepth::TrueColor, KITTY), LEFT_MARGIN)));
    }

    #[test]
    fn ansi256_without_pictures() {
        insta::assert_snapshot!(readable(&ansi(&sample(), &capabilities(ColorDepth::Ansi256, None), LEFT_MARGIN)));
    }

    #[test]
    fn no_colour_is_plain_text() {
        assert_eq!(
            ansi(&sample(), &capabilities(ColorDepth::None, None), LEFT_MARGIN),
            "  Title\n\n  Read the docs or run  mrk .\n  [picture: flow]\n"
        );
    }

    #[test]
    fn a_picture_is_drawn_after_its_indent_on_every_row() {
        let indent = Line::new(vec![Span::plain("│ ")]);
        let picture = Picture { png: vec![1, 2, 3], cols: 20, rows: 2, alt: "flow".to_owned(), indent, concealed_text: None };
        let document = Document { blocks: vec![Block::Picture(picture)] };

        assert_eq!(
            ansi(&document, &capabilities(ColorDepth::None, KITTY), LEFT_MARGIN),
            "  │ \n  │ \n\x1b[2A\r  │ \x1b_Ga=T,f=100,q=2,C=1,c=20,r=2,z=0,m=0;AQID\x1b\\\x1b[2B\r"
        );
        assert_eq!(ansi(&document, &capabilities(ColorDepth::None, None), LEFT_MARGIN), "  │ [picture: flow]\n");
    }

    #[test]
    fn a_picture_taller_than_the_screen_is_drawn_in_runs_the_cursor_can_climb_back_over() {
        let png = sixel::test_png(10, 50, &[[255, 0, 0, 255]; 500]);
        let picture = Picture { png, cols: 1, rows: 5, alt: "flow".to_owned(), indent: Line::blank(), concealed_text: None };
        let short_screen = Capabilities { rows: 3, ..capabilities(ColorDepth::None, KITTY) };

        let drawn = ansi(&Document { blocks: vec![Block::Picture(picture)] }, &short_screen, 0);
        let runs: Vec<&str> = drawn.split_inclusive("B\r").collect();

        assert_eq!(runs.len(), 3, "{drawn:?}");
        assert!(runs[0].starts_with("\n\n\x1b[2A\r\x1b_Ga=T,f=100,q=2,C=1,x=0,y=0,w=10,h=20,c=1,r=2,"), "{drawn:?}");
        assert!(runs[1].starts_with("\n\n\x1b[2A\r\x1b_Ga=T,f=100,q=2,C=1,x=0,y=20,w=10,h=20,c=1,r=2,"), "{drawn:?}");
        assert!(runs[2].starts_with("\n\x1b[1A\r\x1b_Ga=T,f=100,q=2,C=1,x=0,y=40,w=10,h=10,c=1,r=1,"), "{drawn:?}");
        assert!(drawn.ends_with("\x1b[1B\r"), "{drawn:?}");
    }

    const SIXEL: Option<Graphics> = Some(Graphics { protocol: Protocol::Sixel, cell: CELL });

    #[test]
    fn a_tall_sixel_picture_sends_each_run_of_pixel_rows() {
        let png = sixel::test_png(1, 24, &[[255, 0, 0, 255]; 24]);
        let picture = Picture { png, cols: 1, rows: 4, alt: "flow".to_owned(), indent: Line::blank(), concealed_text: None };
        let short_screen = Capabilities { rows: 3, ..capabilities(ColorDepth::None, SIXEL) };

        let drawn = ansi(&Document { blocks: vec![Block::Picture(picture)] }, &short_screen, 0);

        assert_eq!(drawn.matches("\x1b7\x1bP0;1;0q\"1;1;1;12#").count(), 2, "{drawn:?}");
    }

    #[test]
    fn a_sixel_picture_is_drawn_after_its_indent_and_the_cursor_put_back() {
        let picture = Picture {
            png: sixel::test_png(1, 6, &[[255, 0, 0, 255]; 6]),
            cols: 1,
            rows: 1,
            alt: "flow".to_owned(),
            indent: Line::new(vec![Span::plain("│ ")]),
            concealed_text: None,
        };
        let document = Document { blocks: vec![Block::Picture(picture)] };

        assert_eq!(
            ansi(&document, &capabilities(ColorDepth::None, SIXEL), LEFT_MARGIN),
            "  │ \n\x1b[1A\r  │ \x1b7\x1bP0;1;0q\"1;1;1;6#0;2;100;0;0#0~-\x1b\\\x1b8\x1b[1B\r"
        );
    }

    #[test]
    fn inside_tmux_a_picture_is_rows_of_placeholder_cells_after_its_indent() {
        let picture = Picture {
            png: vec![1, 2, 3],
            cols: 2,
            rows: 2,
            alt: "flow".to_owned(),
            indent: Line::new(vec![Span::plain("│ ")]),
            concealed_text: None,
        };
        let document = Document { blocks: vec![Block::Picture(picture)] };
        let through_tmux = Some(Graphics { protocol: Protocol::KittyThroughTmux, cell: CELL });

        let drawn = ansi(&document, &capabilities(ColorDepth::None, through_tmux), LEFT_MARGIN);
        let rows: Vec<&str> = drawn.lines().collect();

        assert!(rows[0].starts_with("\x1bPtmux;") && rows[0].contains("  │ \x1b[38;5;"), "{drawn:?}");
        assert!(rows[1].starts_with("  │ \x1b[38;5;") && rows[1].ends_with("\u{10eeee}\x1b[39m"), "{drawn:?}");
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn an_unreadable_png_shows_the_placeholder_on_a_sixel_terminal() {
        let picture =
            Picture { png: vec![1, 2, 3], cols: 20, rows: 2, alt: "flow".to_owned(), indent: Line::blank(), concealed_text: None };

        assert_eq!(
            ansi(&Document { blocks: vec![Block::Picture(picture)] }, &capabilities(ColorDepth::None, SIXEL), 2),
            "  [picture: flow]\n"
        );
    }

    #[test]
    fn the_placeholder_sanitizes_the_alt_text() {
        let picture =
            Picture { png: vec![], cols: 20, rows: 2, alt: "a\x1b]0;x\x07".to_owned(), indent: Line::blank(), concealed_text: None };

        assert_eq!(placeholder(&picture, "  "), "  [picture: a]0;x]\n");
    }

    #[test]
    fn a_jumbo_title_is_a_picture_with_kitty_double_height_text_where_drawn_and_normal_elsewhere() {
        let double_height = |graphics| Capabilities { double_height: true, ..capabilities(ColorDepth::TrueColor, graphics) };
        let through_tmux = Some(Graphics { protocol: Protocol::KittyThroughTmux, cell: CELL });

        assert_eq!(jumbo_title(&capabilities(ColorDepth::TrueColor, KITTY)), Some(JumboTitle::Picture));
        assert_eq!(jumbo_title(&double_height(KITTY)), Some(JumboTitle::Picture));
        assert_eq!(jumbo_title(&double_height(SIXEL)), Some(JumboTitle::DoubleHeight));
        assert_eq!(jumbo_title(&double_height(None)), Some(JumboTitle::DoubleHeight));
        for graphics in [None, SIXEL, through_tmux] {
            assert_eq!(jumbo_title(&capabilities(ColorDepth::TrueColor, graphics)), None, "{graphics:?}");
        }
    }

    fn jumbo_sample(jumbo_title: Option<JumboTitle>) -> Document {
        let settings = crate::document::Settings { width: 30, jumbo_title, ..crate::theme::test_settings() };
        crate::markdown::render("# Big title\n\nText", &settings)
    }

    #[test]
    fn a_double_height_title_writes_each_line_as_its_two_halves_at_half_the_margin() {
        let capabilities = Capabilities { double_height: true, ..capabilities(ColorDepth::TrueColor, None) };

        insta::assert_snapshot!(readable(&ansi(&jumbo_sample(Some(JumboTitle::DoubleHeight)), &capabilities, 5)));
    }

    #[test]
    fn without_a_jumbo_title_the_heading_is_one_row() {
        insta::assert_snapshot!(readable(&ansi(&jumbo_sample(None), &capabilities(ColorDepth::TrueColor, None), 5)));
    }

    #[test]
    fn a_title_picture_has_its_text_concealed_on_its_first_row_and_is_drawn_under_it() {
        let picture = Picture {
            png: vec![1, 2, 3],
            cols: 20,
            rows: 2,
            alt: "T".to_owned(),
            indent: Line::blank(),
            concealed_text: Some("T\x1b]0;x\x07".to_owned()),
        };

        assert_eq!(
            ansi(&Document { blocks: vec![Block::Picture(picture)] }, &capabilities(ColorDepth::TrueColor, KITTY), LEFT_MARGIN),
            "  \x1b[8mT]0;x\x1b[28m\n  \n\x1b[2A\r  \x1b_Ga=T,f=100,q=2,C=1,c=20,r=2,z=-1,m=0;AQID\x1b\\\x1b[2B\r"
        );
    }

    #[test]
    fn write_produces_the_same_bytes_as_ansi() {
        let capabilities = capabilities(ColorDepth::TrueColor, None);
        let mut out = Vec::new();
        write(&sample(), &capabilities, LEFT_MARGIN, &mut out).unwrap();

        assert_eq!(String::from_utf8(out).unwrap(), ansi(&sample(), &capabilities, LEFT_MARGIN));
    }
}
