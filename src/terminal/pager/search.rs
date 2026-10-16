use std::ops::Range;

use super::page::Row;
use crate::document::{self, Line, Style};
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
            Row::Picture(_) | Row::DoubleHeight(..) => Vec::new(),
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

    /// The same query over `rows`, the page `previous` rendered again at another width. The current match keeps its
    /// number when the count did not change; else it is the one nearest where the current one stood in the text, the
    /// closest number breaking a tie; with none before, the first at or below `from_row`.
    pub fn rewrapped(&self, previous: &[Row], rows: &[Row], from_row: usize) -> Self {
        let search = Self::new(rows, &self.query, from_row);
        let Some(current) = self.current else { return search };
        if search.matches.len() == self.matches.len() {
            return Self { current: Some(current), ..search };
        }
        let target = text_positions(previous, &self.matches[current..=current])[0];
        let positions = text_positions(rows, &search.matches);
        let nearest = (0..positions.len()).min_by_key(|&index| (positions[index].abs_diff(target), index.abs_diff(current)));
        Self { current: nearest.or(search.current), ..search }
    }
}

fn alphanumerics(text: impl Iterator<Item = char>) -> usize {
    text.filter(|character| character.is_alphanumeric()).count()
}

/// How many letters and digits come before each match: wrapping again moves spaces, indents and bars, but keeps the
/// words in order, so this stays about the same for the same match.
fn text_positions(rows: &[Row], matches: &[Match]) -> Vec<usize> {
    let plain = |row: &Row| if let Row::Line(line) = row { line.plain() } else { String::new() };
    let before_row: Vec<usize> = rows
        .iter()
        .scan(0, |total, row| {
            let before = *total;
            *total += alphanumerics(plain(row).chars());
            Some(before)
        })
        .collect();
    matches.iter().map(|found| before_row[found.row] + alphanumerics(plain(&rows[found.row]).chars().take(found.start))).collect()
}

fn matched_style(palette: &Palette) -> Style {
    Style::fg(palette.text).on(palette.subtle)
}

fn current_style(palette: &Palette) -> Style {
    Style::fg(palette.surface).on(palette.accent).bold()
}

/// The line with the matches on it painted: the current one in `accent`, the others on `subtle`. Spans are split
/// where a match starts or ends inside them and keep their link.
pub fn highlight(line: &Line, matches: &[Match], current: Option<Match>, palette: &Palette) -> Line {
    let current = current.filter(|each| matches.contains(each));
    let others: Vec<Range<usize>> = matches.iter().filter(|each| Some(**each) != current).map(|each| each.start..each.end).collect();
    let current: Vec<Range<usize>> = current.into_iter().map(|each| each.start..each.end).collect();
    let painted = document::highlight(std::slice::from_ref(line), &others, |_| matched_style(palette));
    let painted = document::highlight(&painted, &current, |_| current_style(palette));
    painted.into_iter().next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Rgb, Span};
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
        assert_eq!(find(&rows, ""), []);
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
    fn wrapped_again_the_current_match_stays_on_the_same_words_though_others_split() {
        let wide = rows(&["foo bar", "foo bar", "target foo bar"]);
        let narrow = rows(&["foo", "bar foo bar", "target foo bar"]);
        let third = Search { current: Some(2), ..Search::new(&wide, "foo bar", 0) };

        let rewrapped = third.rewrapped(&wide, &narrow, 0);

        assert_eq!(rewrapped.matches.len(), 2);
        assert_eq!(rewrapped.current_match(), Some(Match { row: 2, start: 7, end: 14 }));
        assert_eq!(Search::new(&wide, "zz", 0).rewrapped(&wide, &narrow, 1), Search::new(&narrow, "zz", 1));
    }

    #[test]
    fn wrapped_again_with_as_many_matches_the_current_one_keeps_its_number() {
        let wide = rows(&["-- a --", "-- b"]);
        let narrow = rows(&["--", "a --", "--", "b"]);
        let second = Search { current: Some(1), ..Search::new(&wide, "--", 0) };

        assert_eq!(second.rewrapped(&wide, &narrow, 0).current, Some(1));
    }

    #[test]
    fn a_tie_in_the_text_goes_to_the_closest_number() {
        let wide = rows(&["-- -- --", "x"]);
        let narrow = rows(&["-- -", "- --", "x"]);
        let third = Search { current: Some(2), ..Search::new(&wide, "--", 0) };

        let rewrapped = third.rewrapped(&wide, &narrow, 0);

        assert_eq!(rewrapped.matches.len(), 2);
        assert_eq!(rewrapped.current, Some(1));
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

    #[test]
    fn the_current_match_is_painted_only_on_its_own_row() {
        let palette = MRK_DARK.palette;
        let rows = rows(&["ab", "ab"]);
        let matches = find(&rows, "ab");
        let Row::Line(other_line) = &rows[1] else { unreachable!() };
        let lit = highlight(other_line, &matches[1..], matches.first().copied(), &palette);

        assert_eq!(lit.spans, [Span::new("ab", Style::fg(palette.text).on(palette.subtle))]);
    }
}
