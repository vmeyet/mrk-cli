use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};

use crate::theme::Palette;

/// Enough for every diagram of a long document, small enough that a program rendering many never grows without end.
const CAPACITY: usize = 32;

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
        entries.iter().find(|(known, known_palette, _)| known == source && known_palette == palette).map(|(_, _, value)| value.clone())
    }

    pub fn keep(&self, source: &str, palette: &Palette, value: T) {
        let mut entries = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if entries.len() == CAPACITY {
            entries.pop_front();
        }
        entries.push_back((source.to_owned(), *palette, value));
    }
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
