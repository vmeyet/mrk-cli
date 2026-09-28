use super::page::Row;
use crate::document::{Line, Span, Style};
use crate::theme::Palette;

/// A match of the query in one row, in characters of `Line::plain`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    pub row: usize,
    pub start: usize,
    pub end: usize,
}

/// The confirmed query, its matches in reading order, and the one `n`/`N` moved to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Search {
    pub query: String,
    pub matches: Vec<Match>,
    pub current: Option<usize>,
}

/// One character per character, so indices in the folded text are indices in the original.
fn folded(text: &str) -> Vec<char> {
    text.chars().map(|character| character.to_lowercase().next().unwrap_or(character)).collect()
}

fn line_matches(row: usize, line: &Line, query: &[char]) -> Vec<Match> {
    let text = folded(&line.plain());
    let mut matches = Vec::new();
    let mut start = 0;
    while start + query.len() <= text.len() {
        if text[start..start + query.len()] == *query {
            matches.push(Match { row, start, end: start + query.len() });
            start += query.len();
        } else {
            start += 1;
        }
    }
    matches
}

/// Every case-insensitive, literal, non-overlapping occurrence of `query` in the rows' text.
pub fn find(rows: &[Row], query: &str) -> Vec<Match> {
    let query = folded(query);
    if query.is_empty() {
        return Vec::new();
    }
    rows.iter()
        .enumerate()
        .flat_map(|(index, row)| match row {
            Row::Line(line) => line_matches(index, line, &query),
            Row::Picture(_) => Vec::new(),
        })
        .collect()
}

impl Search {
    /// The query's matches, the current one the first at or below `from_row`, wrapping to the first.
    pub fn new(rows: &[Row], query: &str, from_row: usize) -> Self {
        let matches = find(rows, query);
        let below = matches.iter().position(|each| each.row >= from_row);
        let current = below.or((!matches.is_empty()).then_some(0));
        Self { query: query.to_owned(), matches, current }
    }

    pub fn next(self) -> Self {
        let count = self.matches.len();
        Self { current: self.current.map(|current| (current + 1) % count), ..self }
    }

    pub fn previous(self) -> Self {
        let count = self.matches.len();
        Self { current: self.current.map(|current| (current + count - 1) % count), ..self }
    }

    pub fn current_match(&self) -> Option<Match> {
        self.current.and_then(|current| self.matches.get(current)).copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    None,
    Match,
    Current,
}

fn mark(index: usize, matches: &[Match], current: Option<Match>) -> Mark {
    match matches.iter().find(|each| (each.start..each.end).contains(&index)) {
        None => Mark::None,
        Some(found) if Some(*found) == current => Mark::Current,
        Some(_) => Mark::Match,
    }
}

fn marked_style(style: Style, mark: Mark, palette: &Palette) -> Style {
    match mark {
        Mark::None => style,
        Mark::Match => Style::fg(palette.text).on(palette.subtle),
        Mark::Current => Style::fg(palette.surface).on(palette.accent).bold(),
    }
}

fn split_span(span: &Span, first: usize, matches: &[Match], current: Option<Match>, palette: &Palette) -> Vec<Span> {
    let mut pieces: Vec<(Mark, String)> = Vec::new();
    for (offset, character) in span.text.chars().enumerate() {
        let mark = mark(first + offset, matches, current);
        match pieces.last_mut() {
            Some((last, text)) if *last == mark => text.push(character),
            _ => pieces.push((mark, character.to_string())),
        }
    }
    pieces.into_iter().map(|(mark, text)| Span { text, style: marked_style(span.style, mark, palette), link: span.link.clone() }).collect()
}

/// The line with the matches on it painted: the current one in `accent`, the others on `subtle`. Spans are split
/// where a match starts or ends inside them and keep their link.
pub fn highlight(line: &Line, matches: &[Match], current: Option<Match>, palette: &Palette) -> Line {
    let firsts = line.spans.iter().scan(0, |next, span| {
        let first = *next;
        *next += span.text.chars().count();
        Some(first)
    });
    Line::new(line.spans.iter().zip(firsts).flat_map(|(span, first)| split_span(span, first, matches, current, palette)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Rgb;
    use crate::theme::MRK_DARK;

    const RED: Rgb = Rgb(255, 0, 0);

    fn rows(texts: &[&str]) -> Vec<Row> {
        texts.iter().map(|text| Row::Line(Line::new(vec![Span::plain(*text)]))).collect()
    }

    #[test]
    fn search_is_case_insensitive_literal_and_skips_pictures() {
        let rows = [rows(&["Foo foo", "a.b"]), vec![Row::Picture(0)], rows(&["FOOD"])].concat();

        assert_eq!(
            find(&rows, "foo"),
            [Match { row: 0, start: 0, end: 3 }, Match { row: 0, start: 4, end: 7 }, Match { row: 3, start: 0, end: 3 }]
        );
        assert_eq!(find(&rows, "."), [Match { row: 1, start: 1, end: 2 }]);
        assert!(find(&rows, "").is_empty());
    }

    #[test]
    fn matches_do_not_overlap() {
        assert_eq!(find(&rows(&["aaaa"]), "aa").len(), 2);
    }

    #[test]
    fn indices_count_characters_not_bytes() {
        assert_eq!(find(&rows(&["été Été"]), "été"), [Match { row: 0, start: 0, end: 3 }, Match { row: 0, start: 4, end: 7 }]);
    }

    #[test]
    fn the_current_match_starts_below_the_top_and_wraps_both_ways() {
        let rows = rows(&["x", "a", "x", "x"]);
        let search = Search::new(&rows, "x", 1);

        assert_eq!(search.current, Some(1));
        assert_eq!(search.clone().next().current, Some(2));
        assert_eq!(search.clone().next().next().current, Some(0));
        assert_eq!(search.previous().previous().current, Some(2));
        assert_eq!(Search::new(&rows, "x", 9).current, Some(0));
        assert_eq!(Search::new(&rows, "zz", 0).current, None);
    }

    #[test]
    fn a_match_across_styled_spans_splits_them_and_keeps_links() {
        let palette = MRK_DARK.palette;
        let line =
            Line::new(vec![Span::new("say hel", Style::fg(RED)), Span::new("lo world", Style::fg(RED).bold()).linked("https://x.y")]);
        let matches = find(&[Row::Line(line.clone())], "hello");
        let lit = highlight(&line, &matches, matches.first().copied(), &palette);
        let current = Style::fg(palette.surface).on(palette.accent).bold();

        assert_eq!(
            lit.spans,
            [
                Span::new("say ", Style::fg(RED)),
                Span::new("hel", current),
                Span::new("lo", current).linked("https://x.y"),
                Span::new(" world", Style::fg(RED).bold()).linked("https://x.y"),
            ]
        );
    }

    #[test]
    fn other_matches_are_painted_on_subtle() {
        let palette = MRK_DARK.palette;
        let line = Line::new(vec![Span::plain("ab ab")]);
        let matches = find(&[Row::Line(line.clone())], "ab");
        let lit = highlight(&line, &matches, matches.get(1).copied(), &palette);

        assert_eq!(lit.spans[0], Span::new("ab", Style::fg(palette.text).on(palette.subtle)));
        assert_eq!(lit.spans[2].style.bg, Some(palette.accent));
        assert_eq!(lit.plain(), "ab ab");
    }
}
