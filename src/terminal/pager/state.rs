use super::keys::Action;
use super::page::Page;
use super::search::Search;

/// Everything the pager shows: the page, the first row on screen, how many rows fit, the search and the prompt being typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pager {
    pub page: Page,
    pub top: usize,
    pub height: usize,
    pub search: Search,
    pub prompt: Option<String>,
}

impl Pager {
    pub fn new(page: Page, height: usize) -> Self {
        Self { page, height, ..Self::default() }
    }

    pub fn max_top(&self) -> usize {
        self.page.rows.len().saturating_sub(self.height)
    }

    fn at(self, top: usize) -> Self {
        let top = top.min(self.max_top());
        Self { top, ..self }
    }

    fn down(self, rows: usize) -> Self {
        let top = self.top.saturating_add(rows);
        self.at(top)
    }

    fn up(self, rows: usize) -> Self {
        let top = self.top.saturating_sub(rows);
        self.at(top)
    }

    fn page_rows(&self) -> usize {
        self.height.max(1)
    }

    fn half_rows(&self) -> usize {
        (self.height / 2).max(1)
    }

    /// Scrolls just enough to bring the current match on screen, with a quarter screen of context above it.
    fn reveal_match(self) -> Self {
        let Some(found) = self.search.current_match() else { return self };
        let is_visible = (self.top..self.top + self.height).contains(&found.row);
        let context = self.height / 4;
        if is_visible { self } else { self.at(found.row.saturating_sub(context)) }
    }

    fn type_char(self, character: char) -> Self {
        let prompt = self.prompt.map(|typed| format!("{typed}{character}"));
        Self { prompt, ..self }
    }

    fn erase(self) -> Self {
        let prompt = self.prompt.map(|mut typed| {
            typed.pop();
            typed
        });
        Self { prompt, ..self }
    }

    fn confirm(self) -> Self {
        let query = self.prompt.clone().unwrap_or_default();
        let search = Search::new(&self.page.rows, &query, self.top);
        Self { search, prompt: None, ..self }.reveal_match()
    }

    fn move_match(self, next: fn(Search) -> Search) -> Self {
        let search = next(self.search.clone());
        Self { search, ..self }.reveal_match()
    }

    /// The pager after `action`; `Quit` is for the caller and changes nothing.
    pub fn apply(self, action: &Action) -> Self {
        match action {
            Action::LineDown => self.down(1),
            Action::LineUp => self.up(1),
            Action::PageDown => {
                let rows = self.page_rows();
                self.down(rows)
            }
            Action::PageUp => {
                let rows = self.page_rows();
                self.up(rows)
            }
            Action::HalfDown => {
                let rows = self.half_rows();
                self.down(rows)
            }
            Action::HalfUp => {
                let rows = self.half_rows();
                self.up(rows)
            }
            Action::Top => self.at(0),
            Action::Bottom => self.at(usize::MAX),
            Action::StartSearch => Self { prompt: Some(String::new()), ..self },
            Action::NextMatch => self.move_match(Search::next),
            Action::PreviousMatch => self.move_match(Search::previous),
            Action::Type(character) => self.type_char(*character),
            Action::Erase => self.erase(),
            Action::Confirm => self.confirm(),
            Action::Cancel => Self { prompt: None, ..self },
            Action::Quit => self,
        }
    }

    pub fn with_height(self, height: usize) -> Self {
        let top = self.top;
        Self { height, ..self }.at(top)
    }

    /// The pager over a page rendered again for a new window size, at the same place in the document.
    pub fn resized(self, page: Page, height: usize) -> Self {
        let old_rows = self.page.rows.len().max(1);
        let top = self.top * page.rows.len() / old_rows;
        let current = self.search.current;
        let search = Search::new(&page.rows, &self.search.query, top);
        let current = current.filter(|&index| index < search.matches.len()).or(search.current);
        Self { page, height, search: Search { current, ..search }, ..self }.at(top)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Line, Span};
    use crate::terminal::pager::page::Row;

    fn page(texts: &[&str]) -> Page {
        Page { rows: texts.iter().map(|text| Row::Line(Line::new(vec![Span::plain(*text)]))).collect(), pictures: Vec::new() }
    }

    fn numbered(count: usize) -> Page {
        let texts: Vec<String> = (0..count).map(|index| format!("row {index}")).collect();
        page(&texts.iter().map(String::as_str).collect::<Vec<_>>())
    }

    fn after(pager: Pager, actions: &[Action]) -> Pager {
        actions.iter().fold(pager, Pager::apply)
    }

    #[test]
    fn scrolling_stops_at_both_ends() {
        let pager = Pager::new(numbered(30), 10);

        assert_eq!(after(pager.clone(), &[Action::LineUp]).top, 0);
        assert_eq!(after(pager.clone(), &[Action::LineDown, Action::LineDown]).top, 2);
        assert_eq!(after(pager.clone(), &[Action::PageDown]).top, 10);
        assert_eq!(after(pager.clone(), &[Action::PageDown, Action::PageDown, Action::PageDown]).top, 20);
        assert_eq!(after(pager.clone(), &[Action::HalfDown, Action::HalfDown, Action::HalfUp]).top, 5);
        assert_eq!(after(pager.clone(), &[Action::Bottom]).top, 20);
        assert_eq!(after(pager.clone(), &[Action::Bottom, Action::PageUp, Action::Top]).top, 0);
        assert_eq!(after(pager, &[Action::Bottom, Action::LineDown]).top, 20);
    }

    #[test]
    fn a_page_that_fits_never_scrolls() {
        let pager = Pager::new(numbered(5), 10);

        assert_eq!(after(pager, &[Action::PageDown, Action::Bottom, Action::LineDown]).top, 0);
    }

    #[test]
    fn typing_a_query_then_confirming_jumps_to_the_first_match() {
        let pager = Pager::new(numbered(40), 8);
        let typed = after(pager, &[Action::StartSearch, Action::Type('r'), Action::Type('o'), Action::Type('w'), Action::Type(' ')]);
        let typed = after(typed, &[Action::Type('3'), Action::Type('3'), Action::Erase, Action::Type('1')]);

        assert_eq!(typed.prompt.as_deref(), Some("row 31"));
        let confirmed = after(typed, &[Action::Confirm]);
        assert_eq!(confirmed.prompt, None);
        assert_eq!(confirmed.search.current_match().map(|found| found.row), Some(31));
        assert_eq!(confirmed.top, 29);
    }

    #[test]
    fn next_match_scrolls_it_into_view_with_context() {
        let pager = after(Pager::new(page(&["x", "a", "a", "a", "a", "a", "a", "a", "a", "a", "a", "x"]), 4), &[Action::StartSearch]);
        let searched = after(pager, &[Action::Type('x'), Action::Confirm, Action::NextMatch]);

        assert_eq!(searched.search.current, Some(1));
        assert_eq!(searched.top, 8);
        assert_eq!(after(searched, &[Action::PreviousMatch]).top, 0);
    }

    #[test]
    fn cancel_keeps_the_previous_search() {
        let searched = after(Pager::new(page(&["x"]), 4), &[Action::StartSearch, Action::Type('x'), Action::Confirm]);
        let cancelled = after(searched, &[Action::StartSearch, Action::Type('y'), Action::Cancel]);

        assert_eq!(cancelled.prompt, None);
        assert_eq!(cancelled.search.query, "x");
    }

    #[test]
    fn a_resize_keeps_the_place_in_the_document_and_the_search() {
        let pager = after(Pager::new(numbered(100), 10), &[Action::StartSearch, Action::Type('9'), Action::Confirm]);
        let pager = Pager { top: 50, ..pager };
        let resized = pager.resized(numbered(200), 20);

        assert_eq!(resized.top, 100);
        assert_eq!(resized.height, 20);
        assert_eq!(resized.search.query, "9");
        assert_eq!(resized.search.current, Some(0));
    }
}
