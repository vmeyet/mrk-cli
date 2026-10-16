use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};

use super::budget;
use crate::theme::Palette;

/// Every diagram the budget lets into one document, since each costs at least one statement: a pager resize finds
/// them all, and a program rendering many documents never grows without end.
const CAPACITY: usize = budget::MAX_DOCUMENT_STATEMENTS;

/// The last `CAPACITY` values worked out for a diagram source in a palette, oldest first: the pager renders the
/// document again on every resize, and what does not depend on the width is worked out once.
pub struct Recent<T>(Mutex<VecDeque<(String, Palette, T)>>);

impl<T: Clone> Recent<T> {
    pub const fn new() -> Self {
        Self(Mutex::new(VecDeque::new()))
    }

    /// The value kept for `source` in `palette`, or `work_out`'s, kept for next time; the lock is not held meanwhile.
    pub fn get_or(&self, source: &str, palette: &Palette, work_out: impl FnOnce() -> T) -> T {
        self.find(source, palette).unwrap_or_else(|| {
            let value = work_out();
            self.keep(source, palette, value.clone());
            value
        })
    }

    pub fn find(&self, source: &str, palette: &Palette) -> Option<T> {
        let entries = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        entries.iter().find(|entry| is_for(entry, source, palette)).map(|(_, _, value)| value.clone())
    }

    /// Replaces the value kept for `source` in `palette`, if any.
    pub fn keep(&self, source: &str, palette: &Palette, value: T) {
        let mut entries = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        entries.retain(|entry| !is_for(entry, source, palette));
        if entries.len() == CAPACITY {
            entries.pop_front();
        }
        entries.push_back((source.to_owned(), *palette, value));
    }
}

fn is_for<T>((known, known_palette, _): &(String, Palette, T), source: &str, palette: &Palette) -> bool {
    known == source && known_palette == palette
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{MRK_DARK, MRK_LIGHT};

    #[test]
    fn a_value_is_worked_out_once_per_source_and_palette() {
        let recent = Recent::new();
        let first = recent.get_or("graph TD", &MRK_DARK.palette, || 1);
        let again = recent.get_or("graph TD", &MRK_DARK.palette, || unreachable!("worked out again"));
        let light = recent.get_or("graph TD", &MRK_LIGHT.palette, || 2);

        assert_eq!((first, again, light), (1, 1, 2));
    }

    #[test]
    fn keeping_a_source_again_replaces_its_value() {
        let recent = Recent::new();
        recent.keep("graph TD", &MRK_DARK.palette, 1);
        recent.keep("graph TD", &MRK_DARK.palette, 2);

        assert_eq!(recent.find("graph TD", &MRK_DARK.palette), Some(2));
        assert_eq!(recent.0.lock().unwrap_or_else(PoisonError::into_inner).len(), 1);
    }

    #[test]
    fn every_diagram_of_a_document_at_its_budget_is_worked_out_once_across_renders() {
        let recent = Recent::new();
        let sources: Vec<String> = (0..budget::MAX_DOCUMENT_STATEMENTS).map(|index| format!("n{index}")).collect();
        let worked_out = std::cell::Cell::new(0);

        for _render in 0..3 {
            for source in &sources {
                recent.get_or(source, &MRK_DARK.palette, || worked_out.set(worked_out.get() + 1));
            }
        }

        assert_eq!(worked_out.get(), sources.len());
    }

    #[test]
    fn the_oldest_goes_first_when_full() {
        let recent = Recent::new();
        let sources: Vec<String> = (0..=CAPACITY).map(|index| format!("graph TD\n  n{index}")).collect();
        for (index, source) in sources.iter().enumerate() {
            recent.keep(source, &MRK_DARK.palette, index);
        }

        assert_eq!(recent.find(&sources[0], &MRK_DARK.palette), None);
        assert_eq!(recent.find(&sources[1], &MRK_DARK.palette), Some(1));
        assert_eq!(recent.find(&sources[CAPACITY], &MRK_DARK.palette), Some(CAPACITY));
    }
}
