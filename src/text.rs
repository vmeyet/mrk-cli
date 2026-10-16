use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::document::{Line, Span};
use crate::sanitize::printed;

/// A tab moves to the next column that is a multiple of this.
const TAB_STOP: usize = 4;

/// Terminals draw a grapheme in one or two cells, so a row this wide always has room for the next one.
const WIDEST_GRAPHEME: usize = 2;

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

/// Measured as printed: a tag character that sanitize removes still joins the grapheme before it.
fn grapheme_width(grapheme: &str) -> usize {
    let is_printed_as_written = grapheme.chars().all(|character| printed(character) == Some(character));
    let width = if is_printed_as_written { grapheme.width() } else { grapheme.chars().filter_map(printed).collect::<String>().width() };
    width.min(WIDEST_GRAPHEME)
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
    let tokens = spans.iter().flat_map(|span| span_tokens(span, &span.text, row_tokens));
    place(tokens, width, indent)
}

/// `wrap` for literal text (HTML, front matter): it breaks at spaces but keeps every run of spaces inside a line, and
/// the spaces that open it, and cuts only a word wider than a line. Tabs stop every four columns from the first span.
pub fn wrap_literal(spans: &[Span], width: usize, indent: &[Span]) -> Vec<Line> {
    let texts = expanded_texts(spans);
    let tokens = spans.iter().zip(&texts).flat_map(|(span, text)| span_tokens(span, text, literal_row_tokens));
    place(tokens, width, indent)
}

fn place<'a>(tokens: impl Iterator<Item = Token<'a>>, width: usize, indent: &'a [Span]) -> Vec<Line> {
    let tokens = tokens.fold(Vec::new(), join_words);
    tokens.into_iter().fold(LineBuilder::new(width, indent), LineBuilder::place).finish()
}

/// Each span's text with its tabs expanded, columns counted from the first span.
fn expanded_texts(spans: &[Span]) -> Vec<Cow<'_, str>> {
    let texts = spans.iter().scan(0, |column, span| {
        let text = expand_tabs(&span.text, *column);
        *column += display_width(&text);
        Some(text)
    });
    texts.collect()
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
    let texts = expanded_texts(spans);
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
    Space(Piece<'a>),
    Break,
}

type RowTokens = for<'a> fn(&'a Span, &'a str) -> Vec<Token<'a>>;

/// `text`, the text of `span` as wrapped, in tokens: each `\n` a break, each row cut into tokens by `row_tokens`.
fn span_tokens<'a>(span: &'a Span, text: &'a str, row_tokens: RowTokens) -> impl Iterator<Item = Token<'a>> {
    let rows = text.split('\n').enumerate();
    rows.flat_map(move |(index, row)| (index > 0).then_some(Token::Break).into_iter().chain(row_tokens(span, row)))
}

/// The words and spaces of one row of `span`; on a background the whole row is one word.
fn row_tokens<'a>(span: &'a Span, row: &'a str) -> Vec<Token<'a>> {
    let breaks: &[char] = if span.style.bg.is_some() { &[] } else { &[' ', '\t'] };
    let tokens = row.split(breaks).enumerate().flat_map(move |(index, word)| {
        let space = (index > 0).then_some(Token::Space(Piece { span, text: SPACE }));
        space.into_iter().chain((!word.is_empty()).then(|| Token::Word(word_pieces(span, word))))
    });
    tokens.collect()
}

/// The words and whole runs of spaces of one literal row; the spaces opening it join its first word, so they stay.
fn literal_row_tokens<'a>(span: &'a Span, row: &'a str) -> Vec<Token<'a>> {
    let runs = space_runs(row).into_iter().enumerate();
    let token = |(index, text): (usize, &'a str)| {
        let piece = Piece { span, text };
        if index > 0 && text.starts_with(' ') { Token::Space(piece) } else { Token::Word(vec![piece]) }
    };
    runs.map(token).collect()
}

/// `row` cut into its runs of spaces and the runs between them.
fn space_runs(row: &str) -> Vec<&str> {
    let mut runs = Vec::new();
    let mut rest = row;
    while let Some(first) = rest.chars().next() {
        let is_space = first == ' ';
        let end = rest.find(|character: char| (character == ' ') != is_space).unwrap_or(rest.len());
        let (run, tail) = rest.split_at(end);
        runs.push(run);
        rest = tail;
    }
    runs
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

pub(crate) fn spans_width(spans: &[Span]) -> usize {
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
    space: Option<Piece<'a>>,
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
            Token::Space(space) => self.hold_space(space),
            Token::Word(word) => self.place_word(word),
        }
    }

    fn hold_space(self, space: Piece<'a>) -> Self {
        if self.has_content() { Self { space: Some(space), ..self } } else { self }
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
        current.extend(space);
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
        self.space.map_or(0, Piece::width)
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
    fn a_grapheme_is_as_wide_as_what_sanitize_prints_of_it() {
        let scotland = "\u{1f3f4}\u{e0067}\u{e0062}\u{e0073}\u{e0063}\u{e0074}\u{e007f}";

        assert_eq!(display_width("a\u{e0041}bc"), 3);
        assert_eq!(display_width(scotland), 2);
        assert_eq!(display_width("a\u{2028}b"), 3);
    }

    #[test]
    fn literal_text_keeps_its_runs_of_spaces_and_breaks_at_one() {
        let lines = wrap_literal(&[Span::plain("  <td>a    b</td> <td>c</td>")], 18, &[]);

        assert_eq!(plain_lines(&lines), ["  <td>a    b</td>", "<td>c</td>"]);
        assert_eq!(
            plain_lines(&wrap_literal(&[Span::plain("key:\tvalue averyveryverylongword")], 13, &[])),
            ["key:    value", "averyveryvery", "longword"]
        );
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
