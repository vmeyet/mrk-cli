use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::document::{Line, Span};

/// What a tab becomes on screen.
pub const TAB: &str = "    ";

/// Terminals draw a grapheme in one or two cells, so a row this wide always has room for the next one.
pub const WIDEST_GRAPHEME: usize = 2;

fn is_bidi_control(character: char) -> bool {
    matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// The characters a terminal could interpret: C0 and C1 controls, DEL, bidi overrides and isolates.
/// `terminal::sanitize` removes them, so they take no cell.
pub fn is_forbidden(character: char) -> bool {
    character.is_control() || is_bidi_control(character)
}

/// The cells `text` takes once sanitized, counted one grapheme at a time.
pub fn display_width(text: &str) -> usize {
    if text.bytes().all(|byte| byte == b' ' || byte.is_ascii_graphic()) {
        return text.len();
    }
    text.graphemes(true).map(grapheme_width).sum()
}

fn grapheme_width(grapheme: &str) -> usize {
    match grapheme {
        "\t" => TAB.len(),
        _ if grapheme.chars().any(is_forbidden) => 0,
        _ => grapheme.width().min(WIDEST_GRAPHEME),
    }
}

/// The longest start of `text` that fits in `width` cells, and the rest; a grapheme is never split.
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

/// A run of text that wraps as one unit: a word, a breakable space, or a forced break.
#[derive(Debug, PartialEq)]
enum Token {
    Word(Vec<Span>),
    Space(Span),
    Break,
}

/// The line being filled, and the lines already closed.
struct Layout {
    lines: Vec<Line>,
    current: Vec<Span>,
    used: usize,
    start: usize,
    space: Option<Span>,
}

/// Breaks styled spans into lines of at most `width` cells, at spaces when it can, mid-word when a word is wider than the line.
/// Every line after the first starts with `indent` (hanging indent for list items and quotes).
/// A `\n` forces a break and never reaches a line, a tab counts as a space; a space on a background (inline code padding) holds its word together.
pub fn wrap(spans: &[Span], width: usize, indent: &[Span]) -> Vec<Line> {
    let tokens = tokenize(spans);
    let room = width.saturating_sub(spans_width(indent)).max(1);
    let layout = tokens.into_iter().fold(Layout::new(), |layout, token| layout.place(token, width, room, indent));
    layout.finish()
}

fn tokenize(spans: &[Span]) -> Vec<Token> {
    spans.iter().flat_map(split_span).fold(Vec::new(), |mut tokens, token| {
        match (tokens.last_mut(), token) {
            (Some(Token::Word(word)), Token::Word(pieces)) => word.extend(pieces),
            (_, token) => tokens.push(token),
        }
        tokens
    })
}

fn split_span(span: &Span) -> Vec<Token> {
    let holds_together = span.style.bg.is_some();
    let mut tokens = Vec::new();
    let mut word = String::new();
    for character in span.text.chars().map(|character| if character == '\t' { ' ' } else { character }) {
        let is_break = character == '\n';
        let is_space = character == ' ' && !holds_together;
        if !is_break && !is_space {
            word.push(character);
            continue;
        }
        if !word.is_empty() {
            tokens.push(Token::Word(vec![piece(span, std::mem::take(&mut word))]));
        }
        tokens.push(if is_break { Token::Break } else { Token::Space(piece(span, " ".to_owned())) });
    }
    if !word.is_empty() {
        tokens.push(Token::Word(vec![piece(span, word)]));
    }
    tokens
}

fn piece(span: &Span, text: String) -> Span {
    Span { text, style: span.style, link: span.link.clone() }
}

fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(|span| display_width(&span.text)).sum()
}

impl Layout {
    fn new() -> Self {
        Self { lines: Vec::new(), current: Vec::new(), used: 0, start: 0, space: None }
    }

    fn place(self, token: Token, width: usize, room: usize, indent: &[Span]) -> Self {
        match token {
            Token::Break => self.close(indent),
            Token::Space(space) => self.hold_space(space),
            Token::Word(word) => self.place_word(word, width, room, indent),
        }
    }

    fn hold_space(self, space: Span) -> Self {
        if self.has_content() { Self { space: Some(space), ..self } } else { self }
    }

    fn place_word(self, word: Vec<Span>, width: usize, room: usize, indent: &[Span]) -> Self {
        let word_width = spans_width(&word);
        let fits_here = self.used + self.space_width() + word_width <= width;
        let fits_fresh_line = word_width <= room;
        match (fits_here, fits_fresh_line) {
            (true, _) => self.push_spaced(word, word_width),
            (false, true) if self.has_content() => self.close(indent).push_spaced(word, word_width),
            _ => self.split_word(&word, width, indent),
        }
    }

    fn push_spaced(self, word: Vec<Span>, word_width: usize) -> Self {
        let space_width = self.space_width();
        let Self { lines, mut current, used, start, space } = self;
        current.extend(space);
        current.extend(word);
        Self { lines, current, used: used + space_width + word_width, start, space: None }
    }

    fn split_word(self, word: &[Span], width: usize, indent: &[Span]) -> Self {
        let has_room = width.saturating_sub(self.used + self.space_width()) >= MIN_SPLIT_CELLS;
        let start = match (self.has_content(), has_room) {
            (true, true) => self,
            (true, false) => self.close(indent),
            (false, _) => Self { space: None, ..self },
        };
        word.iter().fold(start, |layout, span| layout.push_cut(span, width, indent))
    }

    /// `span` placed as much as fits at a time, a new line opening whenever the current one is full.
    fn push_cut(self, span: &Span, width: usize, indent: &[Span]) -> Self {
        let mut layout = self;
        let mut rest = span.text.as_str();
        while !rest.is_empty() {
            let free = width.saturating_sub(layout.used + layout.space_width());
            let room = if layout.has_content() { free } else { free.max(WIDEST_GRAPHEME) };
            (layout, rest) = match cut(rest, room) {
                ("", _) => (layout.close(indent), rest),
                (head, tail) => (layout.push_spaced(vec![piece(span, head.to_owned())], display_width(head)), tail),
            };
        }
        layout
    }

    fn close(self, indent: &[Span]) -> Self {
        let Self { mut lines, current, .. } = self;
        lines.push(Line::new(merge(current)));
        let start = spans_width(indent);
        Self { lines, current: indent.to_vec(), used: start, start, space: None }
    }

    fn space_width(&self) -> usize {
        self.space.as_ref().map_or(0, |space| display_width(&space.text))
    }

    fn has_content(&self) -> bool {
        self.used > self.start
    }

    fn finish(self) -> Vec<Line> {
        let Self { mut lines, current, .. } = self;
        lines.push(Line::new(merge(current)));
        lines
    }
}

fn merge(spans: Vec<Span>) -> Vec<Span> {
    spans.into_iter().fold(Vec::new(), |mut merged: Vec<Span>, span| {
        match merged.last_mut() {
            Some(last) if last.style == span.style && last.link == span.link => last.text.push_str(&span.text),
            _ => merged.push(span),
        }
        merged
    })
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
    }

    #[test]
    fn an_emoji_with_its_presentation_selector_is_two_cells() {
        assert_eq!(display_width("❤️"), 2);
        assert_eq!(display_width("👩‍💻"), 2);
    }

    #[test]
    fn what_sanitize_removes_takes_no_cell_and_a_tab_takes_four() {
        assert_eq!(display_width("a\x1b[31mb\u{9b}\u{202e}c\r\n"), 7);
        assert_eq!(display_width("a\tb"), 6);
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
    fn a_wide_character_on_a_one_cell_line_still_moves_forward() {
        let lines = wrap(&[Span::plain("漢字")], 1, &[]);

        assert_eq!(plain_lines(&lines), ["漢", "字"]);
    }

    #[test]
    fn no_line_exceeds_the_width() {
        let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed doeiusmodtemporincididunt ut labore 漢字漢字 et dolore.";
        for width in 3..40 {
            let lines = wrap(&[Span::plain(text)], width, &[Span::plain(" ")]);

            assert!(lines.iter().all(|line| line.width() <= width), "width {width}");
        }
    }
}
