mod ansi;
mod color;
mod detect;
mod environment;
mod kitty;
mod query;
mod reply;
pub mod sanitize;

use std::io::Write;

use serde::Deserialize;

pub use detect::{Preferences, detect};

use crate::document::{Block, CellSize, Document};
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
    /// Set when the terminal draws kitty-protocol pictures and pictures are wanted.
    pub cell: Option<CellSize>,
    pub background: Option<Appearance>,
    pub columns: u16,
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

fn block(block: &Block, capabilities: &Capabilities) -> String {
    match block {
        Block::Lines(lines) => lines.iter().map(|line| ansi::line(line, capabilities)).collect(),
        Block::Picture(picture) if capabilities.cell.is_some() => kitty::picture(picture),
        Block::Picture(picture) => kitty::placeholder(picture),
    }
}

/// Writes the document for this terminal: the only place mrk produces escape sequences.
pub fn write(document: &Document, capabilities: &Capabilities, out: &mut impl Write) -> std::io::Result<()> {
    document.blocks.iter().try_for_each(|each| out.write_all(block(each, capabilities).as_bytes()))
}

/// An error and its causes for stderr, sanitized, causes dimmed when `is_styled`.
pub fn error_report(error: &anyhow::Error, is_styled: bool) -> String {
    let (dim, reset) = if is_styled { ("\x1b[2m", "\x1b[0m") } else { ("", "") };
    let causes: String = error.chain().skip(1).map(|cause| format!("  {dim}{}{reset}\n", sanitize::text(&cause.to_string()))).collect();
    format!("✗ {}\n{causes}", sanitize::text(&error.to_string()))
}

/// The bytes `write` would produce, as a string for tests.
pub fn ansi(document: &Document, capabilities: &Capabilities) -> String {
    document.blocks.iter().map(|each| block(each, capabilities)).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn an_error_report_strips_escapes_and_dims_causes_only_when_styled() {
        let error = anyhow::anyhow!("inner \x1b]52;c;AAAA\x07").context("cannot read \x1b[31mfile");

        assert_eq!(error_report(&error, false), "✗ cannot read [31mfile\n  inner ]52;c;AAAA\n");
        assert_eq!(error_report(&error, true), "✗ cannot read [31mfile\n  \x1b[2minner ]52;c;AAAA\x1b[0m\n");
    }
    use crate::document::{Line, Picture, Span, Style};
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
        let picture = Picture { png: vec![0x89, b'P', b'N', b'G'], cols: 20, rows: 2, alt: "flow".to_owned() };
        Document { blocks: vec![Block::Lines(vec![heading, Line::blank(), prose]), Block::Picture(picture)] }
    }

    fn capabilities(color: ColorDepth, cell: Option<CellSize>) -> Capabilities {
        Capabilities { color, hyperlinks: color != ColorDepth::None, cell, background: None, columns: 80 }
    }

    fn readable(ansi: &str) -> String {
        ansi.replace('\x1b', "␛").replace('\r', "␍")
    }

    #[test]
    fn truecolor_with_links_and_pictures() {
        let cell = Some(CellSize { width_px: 10, height_px: 20 });

        insta::assert_snapshot!(readable(&ansi(&sample(), &capabilities(ColorDepth::TrueColor, cell))));
    }

    #[test]
    fn ansi256_without_pictures() {
        insta::assert_snapshot!(readable(&ansi(&sample(), &capabilities(ColorDepth::Ansi256, None))));
    }

    #[test]
    fn no_colour_is_plain_text() {
        assert_eq!(ansi(&sample(), &capabilities(ColorDepth::None, None)), "  Title\n\n  Read the docs or run  mrk .\n  [picture: flow]\n");
    }

    #[test]
    fn write_produces_the_same_bytes_as_ansi() {
        let capabilities = capabilities(ColorDepth::TrueColor, None);
        let mut out = Vec::new();
        write(&sample(), &capabilities, &mut out).unwrap();

        assert_eq!(String::from_utf8(out).unwrap(), ansi(&sample(), &capabilities));
    }
}
