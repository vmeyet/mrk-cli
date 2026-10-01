mod ansi;
mod color;
mod detect;
mod environment;
mod kitty;
pub mod pager;
mod query;
mod reply;
pub mod sanitize;
mod sixel;

use std::io::Write;

use serde::Deserialize;

pub use detect::{Preferences, detect};

use crate::document::{Block, CellSize, Document, Picture};
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
    pub background: Option<Appearance>,
    pub columns: u16,
    pub is_terminal: bool,
}

/// How the terminal draws pictures; kitty when it speaks both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Kitty,
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

/// What stands left of each row of a picture: the margin, then its indent.
fn picture_indent(picture: &Picture, capabilities: &Capabilities, margin: usize) -> String {
    format!("{}{}", " ".repeat(margin), ansi::line(&picture.indent, capabilities, 0).trim_end_matches('\n'))
}

/// What stands in for a picture, after `indent`, when the terminal cannot draw it.
fn placeholder(picture: &Picture, indent: &str) -> String {
    format!("{indent}[picture: {}]\n", sanitize::text(&picture.alt))
}

/// The picture's rows written first, after `indent` on each, scrolling them into view; then `drawing` at the first of
/// them, which leaves the cursor where it found it; then the cursor at the start of the line below the picture.
/// Drawing at the bottom of the screen must not clip the picture.
fn over_rows(picture: &Picture, indent: &str, drawing: &str) -> String {
    let rows = picture.rows.max(1);
    let reserve = format!("{indent}\n").repeat(usize::from(rows));
    format!("{reserve}\x1b[{rows}A\r{indent}{drawing}\x1b[{rows}B\r")
}

fn picture(picture: &Picture, capabilities: &Capabilities, margin: usize) -> String {
    let indent = picture_indent(picture, capabilities, margin);
    let drawing = capabilities.graphics.and_then(|graphics| match graphics.protocol {
        Protocol::Kitty => Some(kitty::draw(picture)),
        Protocol::Sixel => sixel::draw(picture),
    });
    drawing.map_or_else(|| placeholder(picture, &indent), |drawing| over_rows(picture, &indent, &drawing))
}

fn block(block: &Block, capabilities: &Capabilities, margin: usize) -> String {
    match block {
        Block::Lines(lines) => lines.iter().map(|line| ansi::line(line, capabilities, margin)).collect(),
        Block::Picture(each) => picture(each, capabilities, margin),
    }
}

/// Writes the document for this terminal, `margin` columns from the left: the only place mrk produces escape sequences.
pub fn write(document: &Document, capabilities: &Capabilities, margin: usize, out: &mut impl Write) -> std::io::Result<()> {
    document.blocks.iter().try_for_each(|each| out.write_all(block(each, capabilities, margin).as_bytes()))
}

/// An error and its causes for stderr, sanitized, causes dimmed when `is_styled`.
pub fn error_report(error: &anyhow::Error, is_styled: bool) -> String {
    let (dim, reset) = if is_styled { ("\x1b[2m", "\x1b[0m") } else { ("", "") };
    let causes: String = error.chain().skip(1).map(|cause| format!("  {dim}{}{reset}\n", sanitize::text(&cause.to_string()))).collect();
    format!("✗ {}\n{causes}", sanitize::text(&error.to_string()))
}

/// The bytes `write` would produce, as a string for tests.
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
        let picture = Picture { png: vec![0x89, b'P', b'N', b'G'], cols: 20, rows: 2, alt: "flow".to_owned(), indent: Line::blank() };
        Document { blocks: vec![Block::Lines(vec![heading, Line::blank(), prose]), Block::Picture(picture)] }
    }

    const CELL: CellSize = CellSize { width_px: 10, height_px: 20 };
    const KITTY: Option<Graphics> = Some(Graphics { protocol: Protocol::Kitty, cell: CELL });

    fn capabilities(color: ColorDepth, graphics: Option<Graphics>) -> Capabilities {
        Capabilities { color, hyperlinks: color != ColorDepth::None, graphics, background: None, columns: 80, is_terminal: true }
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
        let picture = Picture { png: vec![1, 2, 3], cols: 20, rows: 2, alt: "flow".to_owned(), indent };
        let document = Document { blocks: vec![Block::Picture(picture)] };

        assert_eq!(
            ansi(&document, &capabilities(ColorDepth::None, KITTY), LEFT_MARGIN),
            "  │ \n  │ \n\x1b[2A\r  │ \x1b_Ga=T,f=100,q=2,C=1,c=20,r=2,m=0;AQID\x1b\\\x1b[2B\r"
        );
        assert_eq!(ansi(&document, &capabilities(ColorDepth::None, None), LEFT_MARGIN), "  │ [picture: flow]\n");
    }

    const SIXEL: Option<Graphics> = Some(Graphics { protocol: Protocol::Sixel, cell: CELL });

    #[test]
    fn a_sixel_picture_is_drawn_after_its_indent_and_the_cursor_put_back() {
        let picture = Picture {
            png: sixel::test_png(1, 6, &[[255, 0, 0, 255]; 6]),
            cols: 1,
            rows: 1,
            alt: "flow".to_owned(),
            indent: Line::new(vec![Span::plain("│ ")]),
        };
        let document = Document { blocks: vec![Block::Picture(picture)] };

        assert_eq!(
            ansi(&document, &capabilities(ColorDepth::None, SIXEL), LEFT_MARGIN),
            "  │ \n\x1b[1A\r  │ \x1b7\x1bP0;1;0q\"1;1;1;6#0;2;100;0;0#0~-\x1b\\\x1b8\x1b[1B\r"
        );
    }

    #[test]
    fn an_unreadable_png_shows_the_placeholder_on_a_sixel_terminal() {
        let picture = Picture { png: vec![1, 2, 3], cols: 20, rows: 2, alt: "flow".to_owned(), indent: Line::blank() };

        assert_eq!(
            ansi(&Document { blocks: vec![Block::Picture(picture)] }, &capabilities(ColorDepth::None, SIXEL), 2),
            "  [picture: flow]\n"
        );
    }

    #[test]
    fn the_placeholder_sanitizes_the_alt_text() {
        let picture = Picture { png: vec![], cols: 20, rows: 2, alt: "a\x1b]0;x\x07".to_owned(), indent: Line::blank() };

        assert_eq!(placeholder(&picture, "  "), "  [picture: a]0;x]\n");
    }

    #[test]
    fn write_produces_the_same_bytes_as_ansi() {
        let capabilities = capabilities(ColorDepth::TrueColor, None);
        let mut out = Vec::new();
        write(&sample(), &capabilities, LEFT_MARGIN, &mut out).unwrap();

        assert_eq!(String::from_utf8(out).unwrap(), ansi(&sample(), &capabilities, LEFT_MARGIN));
    }
}
