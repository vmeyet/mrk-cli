use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::document::{Line, Span};

/// A tab moves to the next column that is a multiple of this.
const TAB_STOP: usize = 4;

/// Terminals draw a grapheme in one or two cells, so a row this wide always has room for the next one.
pub const WIDEST_GRAPHEME: usize = 2;

/// Overrides, isolates and the marks (LRM, RLM, ALM) reorder what a line shows; the zero-width space, the word joiner,
/// the byte order mark and tag characters hide inside a name; the line and paragraph separators break a line the
/// layout never counted. The joiners stay: emoji sequences and Persian or Indic words need them.
fn is_invisible_control(character: char) -> bool {
    matches!(
        character,
        '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
            | '\u{200b}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{061c}'
            | '\u{2060}'
            | '\u{feff}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{e0000}'..='\u{e007f}'
    )
}

/// The characters a terminal could interpret or that disguise text: C0 and C1 controls, DEL and the invisible controls
/// above. `sanitize` removes them, so they take no cell.
pub fn is_forbidden(character: char) -> bool {
    character.is_control() || is_invisible_control(character)
}

/// `text` with each tab written as the spaces up to the next tab stop, `text` starting `column` cells into its line;
/// a newline starts a line again. Borrowed when there is no tab.
pub fn expand_tabs(text: &str, column: usize) -> Cow<'_, str> {
    if !text.contains('\t') {
        return Cow::Borrowed(text);
    }
    let mut expanded = String::with_capacity(text.len() + TAB_STOP);
    let mut column = column;
    for (index, part) in text.split('\t').enumerate() {
        if index > 0 {
            let spaces = tab_width(column);
            expanded.extend(std::iter::repeat_n(' ', spaces));
            column += spaces;
        }
        expanded.push_str(part);
        column = match part.rfind('\n') {
            Some(newline) => display_width(&part[newline + 1..]),
            None => column + display_width(part),
        };
    }
    Cow::Owned(expanded)
}

/// The cells `text` takes once sanitized, counted one grapheme at a time, its tabs expanded from the start of a line.
pub fn display_width(text: &str) -> usize {
    if text.bytes().all(|byte| byte == b' ' || byte.is_ascii_graphic()) {
        return text.len();
    }
    if text.contains('\t') {
        return display_width(&expand_tabs(text, 0));
    }
    text.graphemes(true).map(grapheme_width).sum()
}

fn grapheme_width(grapheme: &str) -> usize {
    if grapheme.chars().any(is_forbidden) { 0 } else { grapheme.width().min(WIDEST_GRAPHEME) }
}

/// The longest start of `text` that fits in `width` cells, and the rest; a grapheme is never split. `text` holds no
/// tab: `expand_tabs` first.
pub fn cut(text: &str, width: usize) -> (&str, &str) {
    let mut used = 0;
    let end = text.grapheme_indices(true).find_map(|(index, grapheme)| {
        used += grapheme_width(grapheme);
        (used > width).then_some(index)
    });
    text.split_at(end.unwrap_or(text.len()))
}

/// A word longer than a whole line starts on the current line only when this much room is left.
const MIN_SPLIT_CELLS: usize = 4;

/// Breaks styled spans into lines of at most `width` cells, at spaces when it can, mid-word when a word is wider than the line.
/// Every line after the first starts with `indent` (hanging indent for list items and quotes).
/// A `\n` forces a break and never reaches a line, a tab breaks like a space; on a background (inline code padding) a
/// space holds its word together and a tab reaches the next tab stop of the line it lands on. `width` is taken as at
/// least the indent plus `WIDEST_GRAPHEME`, so any grapheme fits a line.
pub fn wrap(spans: &[Span], width: usize, indent: &[Span]) -> Vec<Line> {
    let tokens = spans.iter().flat_map(span_tokens).fold(Vec::new(), join_words);
    tokens.into_iter().fold(LineBuilder::new(width, indent), LineBuilder::place).finish()
}

/// Cuts styled spans into lines of at most `width` cells wherever a line is full, never inside a grapheme: for code and
/// source text, which re-flowing at spaces would change. Every line after the first starts with `indent`, tabs stop
/// every four columns from the first span, and `width` is taken as at least the indent plus `WIDEST_GRAPHEME`.
pub fn hard_wrap(spans: &[Span], width: usize, indent: &[Span]) -> Vec<Line> {
    let indent_width = spans_width(indent);
    let width = width.max(indent_width + WIDEST_GRAPHEME);
    let fits = spans_width(spans) <= width && !spans.iter().any(|span| span.text.contains('\t'));
    if fits {
        return vec![Line::joined(pieces_of(spans).map(|piece| (piece.text, piece.span)))];
    }
    let texts: Vec<Cow<str>> = spans
        .iter()
        .scan(0, |column, span| {
            let text = expand_tabs(&span.text, *column);
            *column += display_width(&text);
            Some(text)
        })
        .collect();
    let pieces = spans.iter().zip(&texts).map(|(span, text)| Piece { span, text });
    let rows = cut_rows(pieces, width, width - indent_width);
    let lines = rows.into_iter().enumerate().map(|(index, row)| if index == 0 { row } else { pieces_of(indent).chain(row).collect() });
    lines.map(line_of).collect()
}

/// A slice of a span's text in the span's style: lines are built from these, and own their text only once closed.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Piece<'a> {
    span: &'a Span,
    text: &'a str,
}

impl Piece<'_> {
    fn width(self) -> usize {
        display_width(self.text)
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let (head, tail) = self.text.split_at(index);
        (Self { text: head, ..self }, Self { text: tail, ..self })
    }
}

fn pieces_of(spans: &[Span]) -> impl Iterator<Item = Piece<'_>> {
    spans.iter().map(|span| Piece { span, text: &span.text })
}

fn pieces_width(pieces: &[Piece]) -> usize {
    pieces.iter().map(|piece| piece.width()).sum()
}

fn line_of(pieces: Vec<Piece>) -> Line {
    Line::joined(pieces.into_iter().map(|piece| (piece.text, piece.span)))
}

/// `pieces` cut into rows of whole graphemes, the first at most `first` cells wide and the others `rest`, which must hold
/// any grapheme.
fn cut_rows<'a>(pieces: impl IntoIterator<Item = Piece<'a>>, first: usize, rest: usize) -> Vec<Vec<Piece<'a>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut free = first;
    for mut left in pieces {
        while !left.text.is_empty() {
            match cut(left.text, free) {
                ("", _) => {
                    rows.push(std::mem::take(&mut row));
                    free = rest;
                }
                (head, _) => {
                    let (head, tail) = left.split_at(head.len());
                    free -= head.width();
                    row.push(head);
                    left = tail;
                }
            }
        }
    }
    rows.push(row);
    rows
}

/// A run of text that wraps as one unit: a word, a breakable space in its span's style, or a forced break.
#[derive(Debug, PartialEq)]
enum Token<'a> {
    Word(Vec<Piece<'a>>),
    Space(&'a Span),
    Break,
}

fn span_tokens(span: &Span) -> impl Iterator<Item = Token<'_>> {
    let rows = span.text.split('\n').enumerate();
    rows.flat_map(move |(index, row)| (index > 0).then_some(Token::Break).into_iter().chain(row_tokens(span, row)))
}

/// The words and spaces of one row of `span`; on a background the whole row is one word.
fn row_tokens<'a>(span: &'a Span, row: &'a str) -> impl Iterator<Item = Token<'a>> {
    let breaks: &[char] = if span.style.bg.is_some() { &[] } else { &[' ', '\t'] };
    row.split(breaks).enumerate().flat_map(move |(index, word)| {
        let space = (index > 0).then_some(Token::Space(span));
        space.into_iter().chain((!word.is_empty()).then(|| Token::Word(word_pieces(span, word))))
    })
}

/// A tab in a word is a piece of its own: how wide it is depends on the column the word lands on.
fn word_pieces<'a>(span: &'a Span, word: &'a str) -> Vec<Piece<'a>> {
    let parts = word
        .split('\t')
        .enumerate()
        .flat_map(|(index, text)| (index > 0).then_some(TAB).into_iter().chain((!text.is_empty()).then_some(text)));
    parts.map(|text| Piece { span, text }).collect()
}

const TAB: &str = "\t";
const TAB_SPACES: &str = "    ";
const _: () = assert!(TAB_SPACES.len() == TAB_STOP);

fn tab_width(column: usize) -> usize {
    TAB_STOP - column % TAB_STOP
}

/// The cells `word` takes when it starts at `column`.
fn width_at(word: &[Piece], column: usize) -> usize {
    word.iter().fold(0, |width, piece| width + if piece.text == TAB { tab_width(column + width) } else { piece.width() })
}

/// `word` as it lands at `column`, each tab written as the spaces up to the next tab stop.
fn at_column(word: Vec<Piece<'_>>, column: usize) -> Vec<Piece<'_>> {
    if !word.iter().any(|piece| piece.text == TAB) {
        return word;
    }
    let placed = word.into_iter().scan(column, |column, piece| {
        let text = if piece.text == TAB { &TAB_SPACES[..tab_width(*column)] } else { piece.text };
        *column += display_width(text);
        Some(Piece { text, ..piece })
    });
    placed.collect()
}

/// Words that touch wrap as one, whatever their styles.
fn join_words<'a>(mut tokens: Vec<Token<'a>>, token: Token<'a>) -> Vec<Token<'a>> {
    match (tokens.last_mut(), token) {
        (Some(Token::Word(word)), Token::Word(pieces)) => word.extend(pieces),
        (_, token) => tokens.push(token),
    }
    tokens
}

pub fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(|span| display_width(&span.text)).sum()
}

const SPACE: &str = " ";

/// The line being filled, the lines already closed, and the shape every line takes.
struct LineBuilder<'a> {
    width: usize,
    indent: &'a [Span],
    indent_width: usize,
    lines: Vec<Line>,
    current: Vec<Piece<'a>>,
    used: usize,
    start: usize,
    space: Option<&'a Span>,
}

impl<'a> LineBuilder<'a> {
    fn new(width: usize, indent: &'a [Span]) -> Self {
        let indent_width = spans_width(indent);
        let width = width.max(indent_width + WIDEST_GRAPHEME);
        Self { width, indent, indent_width, lines: Vec::new(), current: Vec::new(), used: 0, start: 0, space: None }
    }

    fn place(self, token: Token<'a>) -> Self {
        match token {
            Token::Break => self.close(),
            Token::Space(span) => self.hold_space(span),
            Token::Word(word) => self.place_word(word),
        }
    }

    fn hold_space(self, span: &'a Span) -> Self {
        if self.has_content() { Self { space: Some(span), ..self } } else { self }
    }

    fn place_word(self, word: Vec<Piece<'a>>) -> Self {
        let column = self.column();
        let width = width_at(&word, column);
        if column + width <= self.width {
            return self.push_spaced(word, width);
        }
        let width = width_at(&word, self.indent_width);
        if self.has_content() && width <= self.room() { self.close().push_spaced(word, width) } else { self.split_word(word) }
    }

    /// `word`, `width` cells wide where it lands, after the space held before it.
    fn push_spaced(self, word: Vec<Piece<'a>>, width: usize) -> Self {
        let column = self.column();
        let word = at_column(word, column);
        let used = column + width;
        let Self { mut current, space, .. } = self;
        current.extend(space.map(|span| Piece { span, text: SPACE }));
        current.extend(word);
        Self { current, used, space: None, ..self }
    }

    /// A word wider than a line, cut wherever a line is full; it starts on this line when enough of it is left.
    fn split_word(self, word: Vec<Piece<'a>>) -> Self {
        let builder = if self.has_content() && self.free() < MIN_SPLIT_CELLS { self.close() } else { self };
        let word = at_column(word, builder.column());
        let mut rows = cut_rows(word, builder.free(), builder.room()).into_iter();
        let first = rows.next().unwrap_or_default();
        let push_row = |builder: Self, row: Vec<Piece<'a>>| {
            let width = pieces_width(&row);
            builder.push_spaced(row, width)
        };
        rows.fold(push_row(builder, first), |builder, row| push_row(builder.close(), row))
    }

    /// Where the next word starts: after the line so far and the space held before the word.
    fn column(&self) -> usize {
        self.used + self.space_width()
    }

    fn close(self) -> Self {
        let Self { mut lines, current, .. } = self;
        lines.push(line_of(current));
        Self { lines, current: pieces_of(self.indent).collect(), used: self.indent_width, start: self.indent_width, space: None, ..self }
    }

    /// The cells a line after the first has for text.
    fn room(&self) -> usize {
        self.width - self.indent_width
    }

    fn free(&self) -> usize {
        self.width.saturating_sub(self.column())
    }

    fn space_width(&self) -> usize {
        if self.space.is_some() { SPACE.len() } else { 0 }
    }

    fn has_content(&self) -> bool {
        self.used > self.start
    }

    fn finish(self) -> Vec<Line> {
        let Self { mut lines, current, .. } = self;
        lines.push(line_of(current));
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Rgb, Style};

    fn plain_lines(lines: &[Line]) -> Vec<String> {
        lines.iter().map(Line::plain).collect()
    }

    #[test]
    fn breaks_at_spaces_and_drops_the_space_at_the_break() {
        let lines = wrap(&[Span::plain("the quick brown fox jumps")], 10, &[]);

        assert_eq!(plain_lines(&lines), ["the quick", "brown fox", "jumps"]);
    }

    #[test]
    fn cuts_a_word_longer_than_the_line() {
        let lines = wrap(&[Span::plain("a supercalifragilistic word")], 8, &[]);

        assert_eq!(plain_lines(&lines), ["a superc", "alifragi", "listic", "word"]);
    }

    #[test]
    fn a_long_word_near_the_end_of_a_line_starts_the_next() {
        let lines = wrap(&[Span::plain("abcdef supercalifragilistic")], 8, &[]);

        assert_eq!(plain_lines(&lines), ["abcdef", "supercal", "ifragili", "stic"]);
    }

    #[test]
    fn hanging_indent_starts_every_line_after_the_first() {
        let lines = wrap(&[Span::plain("one two three four")], 9, &[Span::plain("  ")]);

        assert_eq!(plain_lines(&lines), ["one two", "  three", "  four"]);
    }

    #[test]
    fn counts_wide_characters_as_two_cells() {
        let lines = wrap(&[Span::plain("漢字漢字漢字")], 5, &[]);

        assert_eq!(plain_lines(&lines), ["漢字", "漢字", "漢字"]);
    }

    #[test]
    fn keeps_styles_across_the_break() {
        let bold = Style::default().bold();
        let lines = wrap(&[Span::plain("plain "), Span::new("bold words", bold)], 10, &[]);

        assert_eq!(lines[1].spans, [Span::new("words", bold)]);
    }

    #[test]
    fn a_word_spanning_two_styles_stays_whole() {
        let lines = wrap(&[Span::plain("aaaa bb"), Span::new("cc", Style::default().bold())], 6, &[]);

        assert_eq!(plain_lines(&lines), ["aaaa", "bbcc"]);
    }

    #[test]
    fn newline_forces_a_break_and_never_reaches_a_span() {
        let lines = wrap(&[Span::plain("one\ntwo")], 80, &[]);

        assert_eq!(plain_lines(&lines), ["one", "two"]);
    }

    #[test]
    fn spaces_on_a_background_hold_together() {
        let code = Style::default().on(Rgb(0, 0, 0));
        let lines = wrap(&[Span::plain("see "), Span::new(" a b ", code)], 8, &[]);

        assert_eq!(plain_lines(&lines), ["see", " a b "]);
    }

    #[test]
    fn empty_input_is_one_empty_line() {
        assert_eq!(wrap(&[], 10, &[]), [Line::blank()]);
        assert_eq!(hard_wrap(&[], 10, &[Span::plain("> ")]), [Line::blank()]);
    }

    #[test]
    fn an_emoji_with_its_presentation_selector_is_two_cells() {
        assert_eq!(display_width("❤️"), 2);
        assert_eq!(display_width("👩‍💻"), 2);
    }

    #[test]
    fn what_sanitize_removes_takes_no_cell_and_a_tab_reaches_the_next_stop() {
        assert_eq!(display_width("a\x1b[31mb\u{9b}\u{202e}c\r\n"), 7);
        assert_eq!(display_width("a\tb"), 5);
        assert_eq!(display_width("abcd\tb"), 9);
    }

    #[test]
    fn tabs_stop_every_four_columns_from_the_start_of_the_line() {
        assert_eq!(expand_tabs("ab\tc", 0), "ab  c");
        assert_eq!(expand_tabs("\tc", 2), "  c");
        assert_eq!(expand_tabs("a\nb\tc", 0), "a\nb   c");
        assert!(matches!(expand_tabs("no tab", 0), Cow::Borrowed(_)));
    }

    #[test]
    fn hard_wrap_counts_tab_stops_across_spans() {
        let lines = hard_wrap(&[Span::plain("ab"), Span::new("\tc", Style::default().bold())], 20, &[]);

        assert_eq!(plain_lines(&lines), ["ab  c"]);
    }

    #[test]
    fn cut_keeps_whole_graphemes_that_fit() {
        assert_eq!(cut("❤️❤️❤️", 5), ("❤️❤️", "❤️"));
        assert_eq!(cut("e\u{301}xyz", 2), ("e\u{301}x", "yz"));
        assert_eq!(cut("漢字", 1), ("", "漢字"));
        assert_eq!(cut("abc", 10), ("abc", ""));
    }

    #[test]
    fn emoji_presentation_sequences_never_overflow_the_line() {
        let lines = wrap(&[Span::plain("❤️".repeat(30))], 20, &[]);

        assert_eq!(lines.iter().map(Line::width).collect::<Vec<_>>(), [20, 20, 20]);
    }

    #[test]
    fn a_width_too_narrow_for_the_indent_and_a_wide_grapheme_widens_to_fit_them() {
        assert_eq!(plain_lines(&wrap(&[Span::plain("漢字")], 1, &[])), ["漢", "字"]);
        assert_eq!(plain_lines(&hard_wrap(&[Span::plain("漢字")], 1, &[])), ["漢", "字"]);
        assert_eq!(plain_lines(&wrap(&[Span::plain("ab cd")], 1, &[Span::plain(" ")])), ["ab", " cd"]);
    }

    #[test]
    fn a_tab_is_four_spaces_and_breaks_like_one() {
        let code = Style::default().on(Rgb(0, 0, 0));

        assert_eq!(plain_lines(&wrap(&[Span::plain("one\ttwo")], 6, &[])), ["one", "two"]);
        assert_eq!(plain_lines(&wrap(&[Span::new("\tx", code)], 20, &[])), ["    x"]);
        assert_eq!(plain_lines(&hard_wrap(&[Span::plain("\tabc")], 6, &[])), ["    ab", "c"]);
    }

    #[test]
    fn a_tab_on_a_background_stops_at_the_column_of_the_line_it_lands_on() {
        let code = Style::default().on(Rgb(0, 0, 0));

        assert_eq!(plain_lines(&wrap(&[Span::plain("ab "), Span::new("\tx", code)], 20, &[])), ["ab  x"]);
        assert_eq!(plain_lines(&wrap(&[Span::plain("abcdef "), Span::new("\tx", code)], 8, &[Span::plain(" ")])), ["abcdef", "    x"]);
    }

    #[test]
    fn hard_wrap_fills_each_line_and_indents_the_rest() {
        let bold = Style::default().bold();
        let lines = hard_wrap(&[Span::plain("let a = "), Span::new("12345", bold)], 6, &[Span::plain("> ")]);

        assert_eq!(plain_lines(&lines), ["let a ", "> = 12", "> 345"]);
        assert_eq!(lines[1].spans, [Span::plain("> = "), Span::new("12", bold)]);
    }

    #[test]
    fn no_line_exceeds_the_width() {
        let text = "Lorem ipsum dolor sit amet, consectetur \u{200b} adipiscing elit, sed doeiusmodtemporincididunt ut labore 漢字漢字 et\tdolore.";
        for width in 3..40 {
            let wrapped = wrap(&[Span::plain(text)], width, &[Span::plain(" ")]);
            let cut = hard_wrap(&[Span::plain(text)], width, &[Span::plain(" ")]);

            assert!(wrapped.iter().chain(&cut).all(|line| line.width() <= width), "width {width}");
        }
    }
}
