use std::ops::Range;
use std::time::Instant;

use crate::sanitize;
use crate::theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// `#rrggbb`, as SVG and CSS write a colour.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// The colour `amount` of the way from this one to `other`: 0.0 keeps it, 1.0 gives `other`.
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a float `as u8` saturates into 0..=255")]
    pub fn mix(self, other: Rgb, amount: f32) -> Rgb {
        let channel = |from: u8, to: u8| (f32::from(from) + (f32::from(to) - f32::from(from)) * amount).round() as u8;
        Rgb(channel(self.0, other.0), channel(self.1, other.1), channel(self.2, other.2))
    }
}

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
        Self::default().with_fg(color)
    }

    pub fn with_fg(self, color: Rgb) -> Self {
        Self { fg: Some(color), ..self }
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
    /// Drawn by mrk around the input's text (a bar, a border, a marker, padding), never written by the input: what a
    /// diff leaves out.
    pub is_frame: bool,
}

impl Span {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self { text: text.into(), style, link: None, is_frame: false }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self::new(text, Style::default())
    }

    pub fn linked(self, target: impl Into<String>) -> Self {
        Self { link: Some(target.into()), ..self }
    }

    pub fn framed(self) -> Self {
        Self { is_frame: true, ..self }
    }

    /// The span's look and role, all but its text: what decides whether two neighbours can be one span.
    fn looks_like(&self, other: &Span) -> bool {
        self.style == other.style && self.link == other.link && self.is_frame == other.is_frame
    }

    /// This span with its look and role but no text, to build a span of the same kind from.
    fn without_text(&self) -> Self {
        Self { text: String::new(), style: self.style, link: self.link.clone(), is_frame: self.is_frame }
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

    /// A line of text slices, each styled and linked as its span, neighbours that look alike joined into one span.
    /// Every span and text is allocated at its exact size: documents hold millions of lines, and growth by doubling
    /// would leave up to half of each spare.
    pub fn joined<'a>(pieces: impl IntoIterator<Item = (&'a str, &'a Span)>) -> Self {
        let pieces: Vec<(&str, &Span)> = pieces.into_iter().collect();
        let runs = pieces.chunk_by(|(_, left), (_, right)| left.looks_like(right));
        let mut spans = Vec::with_capacity(runs.clone().count());
        spans.extend(runs.map(|run| {
            let mut text = String::with_capacity(run.iter().map(|(piece, _)| piece.len()).sum());
            text.extend(run.iter().map(|(piece, _)| *piece));
            let (_, first) = run[0];
            Span { text, ..first.without_text() }
        }));
        Self::new(spans)
    }

    pub fn blank() -> Self {
        Self::default()
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(|span| crate::text::display_width(&span.text)).sum()
    }

    /// The text as the input wrote it, escape sequences included: for searching and comparing, never for printing.
    pub fn plain(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    /// The text safe to write to a terminal: `plain` through [`sanitize::text`].
    pub fn printable(&self) -> String {
        sanitize::text(&self.plain())
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
    /// Text written concealed in the picture's first row, under the picture, so selecting the picture copies it.
    pub concealed_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Block {
    Lines(Vec<Line>),
    Picture(Picture),
    /// Lines twice as tall and wide, each written on two rows as its top and bottom half.
    DoubleHeight(Vec<Line>),
}

impl Block {
    /// The text lines of the block, none for a picture.
    pub fn lines(&self) -> &[Line] {
        match self {
            Block::Lines(lines) | Block::DoubleHeight(lines) => lines,
            Block::Picture(_) => &[],
        }
    }
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

/// How a level-1 heading is drawn two rows tall, when asked for and the terminal can.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumboTitle {
    /// A picture of each line over its concealed text.
    Picture,
    /// Double-height text (DECDHL).
    DoubleHeight,
}

/// The narrowest width the renderers lay out at: a code panel needs its padding, its continuation marker and a wide
/// grapheme side by side. A narrower `Settings::width` is laid out this wide.
pub const MIN_WIDTH: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub width: usize,
    pub theme: Theme,
    pub cell: Option<CellSize>,
    /// Whether the terminal makes links clickable; when it does not, a link's target is printed after its text.
    pub hyperlinks: bool,
    pub jumbo_title: Option<JumboTitle>,
    /// When diagrams stop being laid out: each runs on a thread of its own, given up on at that moment, and those
    /// left show as source. `None` lays every diagram out on the calling thread, bounded by the diagram budget alone.
    pub diagram_deadline: Option<Instant>,
}

/// 80 columns of `mrk-dark` text: no pictures, links not clickable (their target printed after them), no jumbo title, no
/// diagram deadline.
impl Default for Settings {
    fn default() -> Self {
        Self { width: 80, theme: crate::theme::MRK_DARK, cell: None, hyperlinks: false, jumbo_title: None, diagram_deadline: None }
    }
}

/// The settings unit tests render with: the defaults, links clickable.
#[cfg(test)]
pub(crate) fn test_settings() -> Settings {
    Settings { hyperlinks: true, ..Settings::default() }
}

/// The document as bare text, one `[picture: alt cols×rows]` line per picture and each double-height line twice:
/// what snapshot tests compare. It keeps the input's escape sequences, so never print it; print `printable` instead.
pub fn plain(document: &Document) -> String {
    document.blocks.iter().map(plain_block).collect()
}

/// `plain` safe to write to a terminal: through [`sanitize::lines`].
pub fn printable(document: &Document) -> String {
    sanitize::lines(&plain(document))
}

fn plain_block(block: &Block) -> String {
    match block {
        Block::Lines(lines) => lines.iter().map(|line| format!("{}\n", line.plain().trim_end())).collect(),
        Block::Picture(picture) => format!("{}[picture: {} {}×{}]\n", picture.indent.plain(), picture.alt, picture.cols, picture.rows),
        Block::DoubleHeight(lines) => lines.iter().map(|line| format!("{0}\n{0}\n", line.plain().trim_end())).collect(),
    }
}

/// `span` cut where a range starts or ends inside it, the pieces inside restyled; `first` is the offset of its first
/// char, `ranges` are `normalized`. A span no range touches comes back whole.
fn split_span(span: &Span, first: usize, ranges: &[Range<usize>], restyle: &impl Fn(Style) -> Style) -> Vec<Span> {
    let end = first + span.text.chars().count();
    let touching = ranges[ranges.partition_point(|range| range.end <= first)..].iter().take_while(|range| range.start < end);
    let piece = |text: &str, style: Style| Span { text: text.to_owned(), style, ..span.without_text() };
    let mut pieces = Vec::new();
    let (mut at, mut rest) = (first, span.text.as_str());
    for range in touching.map(|range| range.start.max(first)..range.end.min(end)) {
        let (outside, tail) = split_chars(rest, range.start - at);
        let (inside, tail) = split_chars(tail, range.len());
        pieces.extend((!outside.is_empty()).then(|| piece(outside, span.style)));
        pieces.push(piece(inside, restyle(span.style)));
        (at, rest) = (range.end, tail);
    }
    if pieces.is_empty() || !rest.is_empty() {
        pieces.push(piece(rest, span.style));
    }
    pieces
}

fn split_chars(text: &str, count: usize) -> (&str, &str) {
    text.split_at(text.char_indices().nth(count).map_or(text.len(), |(index, _)| index))
}

/// The ranges sorted, the ones that overlap or touch merged and the empty ones dropped, so cuts never repeat.
fn normalized(ranges: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut sorted: Vec<Range<usize>> = ranges.iter().filter(|range| !range.is_empty()).cloned().collect();
    sorted.sort_unstable_by_key(|range| range.start);
    sorted.into_iter().fold(Vec::new(), |mut merged: Vec<Range<usize>>, range| {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
        merged
    })
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
/// Ranges are char offsets into the lines' plain text joined with no separator,
/// `lines.iter().map(Line::plain).collect::<String>()`, in any order; overlapping ones restyle once. A span is split
/// where a range starts or ends inside it.
pub fn highlight(lines: &[Line], ranges: &[Range<usize>], restyle: impl Fn(Style) -> Style) -> Vec<Line> {
    let ranges = normalized(ranges);
    let mut offsets = span_offsets(lines);
    lines
        .iter()
        .map(|line| {
            let spans = line.spans.iter().zip(offsets.by_ref()).flat_map(|(span, first)| split_span(span, first, &ranges, &restyle));
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
    fn printable_text_carries_no_escape_sequence_while_plain_keeps_the_input() {
        let line = Line::new(vec![Span::plain("hi \x1b]52;c;AAAA\x07"), Span::plain(" there")]);

        assert_eq!(line.plain(), "hi \x1b]52;c;AAAA\x07 there");
        assert_eq!(line.printable(), "hi ]52;c;AAAA there");
    }

    #[test]
    fn a_printable_document_keeps_its_line_breaks_and_loses_escape_sequences() {
        let line = Line::new(vec![Span::plain("a\x1b[2J")]);
        let document = Document { blocks: vec![Block::Lines(vec![line.clone(), line])] };

        assert_eq!(printable(&document), "a[2J\na[2J\n");
    }

    #[test]
    fn mix_moves_each_channel_toward_the_other_colour_and_rounds() {
        let black = Rgb(0, 0, 0);

        assert_eq!(black.mix(RED, 0.0), black);
        assert_eq!(black.mix(RED, 1.0), RED);
        assert_eq!(Rgb(10, 200, 0).mix(Rgb(20, 100, 255), 0.25), Rgb(13, 175, 64));
    }

    #[test]
    fn plain_joins_spans_and_trims_trailing_padding() {
        let line = Line::new(vec![Span::plain("a "), Span::plain("b   ")]);
        let document = Document { blocks: vec![Block::Lines(vec![line, Line::blank()])] };

        assert_eq!(plain(&document), "a b\n\n");
    }

    #[test]
    fn plain_draws_a_picture_after_its_indent() {
        let picture = Picture {
            png: Vec::new(),
            cols: 4,
            rows: 2,
            alt: "flow".to_owned(),
            indent: Line::new(vec![Span::plain("│ ")]),
            concealed_text: None,
        };

        assert_eq!(plain(&Document { blocks: vec![Block::Picture(picture)] }), "│ [picture: flow 4×2]\n");
    }

    #[test]
    fn plain_writes_a_double_height_line_on_its_two_rows() {
        let document = Document { blocks: vec![Block::DoubleHeight(vec![Line::new(vec![Span::plain("Big ")])])] };

        assert_eq!(plain(&document), "Big\nBig\n");
    }

    #[test]
    fn hex_writes_two_digits_per_channel() {
        assert_eq!(Rgb(0, 15, 255).hex(), "#000fff");
    }

    #[test]
    fn joined_merges_neighbours_of_the_same_look_into_spans_of_exact_size() {
        let (plain, red) = (Span::plain("a"), Span::new("b", Style::fg(RED)));

        let line = Line::joined([("ab", &plain), ("c", &plain), ("d", &red), ("e", &plain)]);

        assert_eq!(line.spans, [Span::plain("abc"), Span::new("d", Style::fg(RED)), Span::plain("e")]);
        assert_eq!((line.spans.capacity(), line.spans[0].text.capacity()), (3, 3));
    }

    #[test]
    fn joined_keeps_neighbours_with_different_links_apart() {
        let (plain, linked) = (Span::plain("a"), Span::plain("b").linked("https://x.y"));

        let line = Line::joined([("a", &plain), ("b", &linked), ("c", &linked)]);

        assert_eq!(line.spans, [Span::plain("a"), Span::plain("bc").linked("https://x.y")]);
        assert_eq!(Line::joined([("a", &plain), ("b", &plain.clone().framed())]).spans.len(), 2);
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
    fn neighbouring_ranges_restyle_as_one_piece_and_a_range_over_a_whole_span_keeps_it_whole() {
        let line = Line::new(vec![Span::plain("abcd"), Span::plain("ef")]);

        let lit = highlight(&[line], &[1..2, 2..3, 4..6], Style::bold);

        assert_eq!(
            lit[0].spans,
            [Span::plain("a"), Span::new("bc", Style::default().bold()), Span::plain("d"), Span::new("ef", Style::default().bold())]
        );
    }

    #[test]
    fn ranges_in_any_order_overlapping_or_repeated_cut_each_edge_once() {
        let lines = [Line::new(vec![Span::plain("abcdef")])];
        let bold = Style::default().bold();

        let lit = highlight(&lines, &[4..5, 1..3, 2..3, 1..3, 5..5], Style::bold);

        assert_eq!(lit[0].spans, [Span::plain("a"), Span::new("bc", bold), Span::plain("d"), Span::new("e", bold), Span::plain("f")]);
    }

    #[test]
    fn an_empty_range_changes_nothing() {
        let lines = [Line::new(vec![Span::plain("abc")])];

        assert_eq!(highlight(&lines, &[1..1], Style::bold), lines);
    }
}
