use std::ops::Range;

use crate::theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub dim: bool,
}

impl Style {
    pub fn fg(color: Rgb) -> Self {
        Self { fg: Some(color), ..Self::default() }
    }

    pub fn on(self, color: Rgb) -> Self {
        Self { bg: Some(color), ..self }
    }

    pub fn bold(self) -> Self {
        Self { bold: true, ..self }
    }

    pub fn italic(self) -> Self {
        Self { italic: true, ..self }
    }

    pub fn underline(self) -> Self {
        Self { underline: true, ..self }
    }

    pub fn strike(self) -> Self {
        Self { strike: true, ..self }
    }

    pub fn dim(self) -> Self {
        Self { dim: true, ..self }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
    pub link: Option<String>,
}

impl Span {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self { text: text.into(), style, link: None }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self::new(text, Style::default())
    }

    pub fn linked(self, target: impl Into<String>) -> Self {
        Self { link: Some(target.into()), ..self }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub spans: Vec<Span>,
}

impl Line {
    pub fn new(spans: Vec<Span>) -> Self {
        Self { spans }
    }

    pub fn blank() -> Self {
        Self::default()
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(|span| crate::text::display_width(&span.text)).sum()
    }

    pub fn plain(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

/// A PNG and the terminal cell box it is drawn into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub png: Vec<u8>,
    pub cols: u16,
    pub rows: u16,
    pub alt: String,
    /// The bars and spaces of the lists and quotes around the picture, drawn left of each of its rows; the picture
    /// starts right after it.
    pub indent: Line,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Lines(Vec<Line>),
    Picture(Picture),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Document {
    pub blocks: Vec<Block>,
}

/// Size of one terminal cell in pixels; present only when pictures can be drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellSize {
    pub width_px: u16,
    pub height_px: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub width: usize,
    pub theme: Theme,
    pub cell: Option<CellSize>,
    /// Whether the terminal makes links clickable; when it does not, a link's target is printed after its text.
    pub hyperlinks: bool,
}

/// The document as bare text, one `[picture: alt cols×rows]` line per picture: what snapshot tests compare.
pub fn plain(document: &Document) -> String {
    document.blocks.iter().map(plain_block).collect()
}

fn plain_block(block: &Block) -> String {
    match block {
        Block::Lines(lines) => lines.iter().map(|line| format!("{}\n", line.plain().trim_end())).collect(),
        Block::Picture(picture) => format!("{}[picture: {} {}×{}]\n", picture.indent.plain(), picture.alt, picture.cols, picture.rows),
    }
}

fn split_span(span: &Span, first: usize, ranges: &[Range<usize>], restyle: &impl Fn(Style) -> Style) -> Vec<Span> {
    let is_inside = |offset: usize| ranges.iter().any(|range| range.contains(&offset));
    let mut pieces: Vec<(bool, String)> = Vec::new();
    for (offset, character) in span.text.chars().enumerate() {
        let inside = is_inside(first + offset);
        match pieces.last_mut() {
            Some((last, text)) if *last == inside => text.push(character),
            _ => pieces.push((inside, character.to_string())),
        }
    }
    let style = |inside: bool| if inside { restyle(span.style) } else { span.style };
    pieces.into_iter().map(|(inside, text)| Span { text, style: style(inside), link: span.link.clone() }).collect()
}

/// The char offset in `plain` text at which each span starts, spans and lines taken in order with nothing between them.
fn span_offsets(lines: &[Line]) -> impl Iterator<Item = usize> {
    lines.iter().flat_map(|line| &line.spans).scan(0, |next, span| {
        let first = *next;
        *next += span.text.chars().count();
        Some(first)
    })
}

/// Restyles the text inside `ranges`, for a search match or a word-level diff; text, widths and links never change.
///
/// Ranges are char offsets into the lines' plain text joined with no separator:
/// `lines.iter().map(Line::plain).collect::<String>()`. A span is split where a range starts or ends inside it.
pub fn highlight(lines: &[Line], ranges: &[Range<usize>], restyle: impl Fn(Style) -> Style) -> Vec<Line> {
    let mut offsets = span_offsets(lines);
    lines
        .iter()
        .map(|line| {
            let spans = line.spans.iter().zip(offsets.by_ref()).flat_map(|(span, first)| split_span(span, first, ranges, &restyle));
            Line::new(spans.collect())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::single_range_in_vec_init)]

    use super::*;

    const RED: Rgb = Rgb(255, 0, 0);

    #[test]
    fn plain_joins_spans_and_trims_trailing_padding() {
        let line = Line::new(vec![Span::plain("a "), Span::plain("b   ")]);
        let document = Document { blocks: vec![Block::Lines(vec![line, Line::blank()])] };

        assert_eq!(plain(&document), "a b\n\n");
    }

    #[test]
    fn plain_draws_a_picture_after_its_indent() {
        let picture = Picture { png: Vec::new(), cols: 4, rows: 2, alt: "flow".to_owned(), indent: Line::new(vec![Span::plain("│ ")]) };

        assert_eq!(plain(&Document { blocks: vec![Block::Picture(picture)] }), "│ [picture: flow 4×2]\n");
    }

    #[test]
    fn line_width_counts_cells_not_bytes() {
        let line = Line::new(vec![Span::plain("é漢")]);

        assert_eq!(line.width(), 3);
    }

    #[test]
    fn highlight_splits_spans_at_the_range_edges_and_keeps_links() {
        let line =
            Line::new(vec![Span::new("say hel", Style::fg(RED)), Span::new("lo world", Style::fg(RED).bold()).linked("https://x.y")]);

        let lit = highlight(&[line], &[4..9], Style::underline);

        assert_eq!(
            lit[0].spans,
            [
                Span::new("say ", Style::fg(RED)),
                Span::new("hel", Style::fg(RED).underline()),
                Span::new("lo", Style::fg(RED).bold().underline()).linked("https://x.y"),
                Span::new(" world", Style::fg(RED).bold()).linked("https://x.y"),
            ]
        );
    }

    #[test]
    fn highlight_crosses_a_line_break_without_changing_the_text() {
        let lines = [Line::new(vec![Span::plain("one two")]), Line::new(vec![Span::plain("three")])];

        let lit = highlight(&lines, &[4..9], Style::bold);

        assert_eq!(lit[0].spans, [Span::plain("one "), Span::new("two", Style::default().bold())]);
        assert_eq!(lit[1].spans, [Span::new("th", Style::default().bold()), Span::plain("ree")]);
        assert_eq!(lit.iter().map(Line::width).collect::<Vec<_>>(), [7, 5]);
    }

    #[test]
    fn an_empty_range_changes_nothing() {
        let lines = [Line::new(vec![Span::plain("abc")])];

        assert_eq!(highlight(&lines, &[1..1], Style::bold), lines);
    }
}
