use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex, PoisonError};

use super::budget;
use crate::theme::Palette;

/// Every diagram the budget lets into one document: a pager resize finds them all, and a program rendering many
/// documents never grows without end.
const CAPACITY: usize = budget::MAX_DOCUMENT_DIAGRAMS;

/// The last `CAPACITY` values worked out for a diagram source in a palette: the pager renders the document again on
/// every resize, and what does not depend on the width is worked out once.
pub struct Recent<T>(LazyLock<Mutex<Entries<T>>>);

/// By palette then source, so a lookup borrows the source instead of copying it.
struct Entries<T> {
    values: HashMap<Palette, HashMap<String, T>>,
    /// Oldest first.
    order: VecDeque<(Palette, String)>,
}

impl<T: Clone> Recent<T> {
    pub const fn new() -> Self {
        Self(LazyLock::new(|| Mutex::new(Entries { values: HashMap::new(), order: VecDeque::new() })))
    }

    pub fn find(&self, source: &str, palette: &Palette) -> Option<T> {
        let entries = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        entries.values.get(palette)?.get(source).cloned()
    }

    /// Replaces the value kept for `source` in `palette`, if any.
    pub fn keep(&self, source: &str, palette: &Palette, value: T) {
        let mut entries = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if entries.values.entry(*palette).or_default().insert(source.to_owned(), value).is_some() {
            return;
        }
        entries.order.push_back((*palette, source.to_owned()));
        if entries.order.len() > CAPACITY
            && let Some((palette, source)) = entries.order.pop_front()
            && let Some(sources) = entries.values.get_mut(&palette)
        {
            sources.remove(&source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{MRK_DARK, MRK_LIGHT};

    #[test]
    fn a_value_is_kept_per_source_and_palette() {
        let recent = Recent::new();
        recent.keep("graph TD", &MRK_DARK.palette, 1);
        recent.keep("graph TD", &MRK_LIGHT.palette, 2);

        assert_eq!(recent.find("graph TD", &MRK_DARK.palette), Some(1));
        assert_eq!(recent.find("graph TD", &MRK_LIGHT.palette), Some(2));
        assert_eq!(recent.find("graph LR", &MRK_DARK.palette), None);
    }

    #[test]
    fn keeping_a_source_again_replaces_its_value() {
        let recent = Recent::new();
        recent.keep("graph TD", &MRK_DARK.palette, 1);
        recent.keep("graph TD", &MRK_DARK.palette, 2);

        assert_eq!(recent.find("graph TD", &MRK_DARK.palette), Some(2));
        assert_eq!(recent.0.lock().unwrap_or_else(PoisonError::into_inner).order.len(), 1);
    }

    #[test]
    fn every_diagram_a_document_lets_through_stays_kept() {
        let recent = Recent::new();
        let sources: Vec<String> = (0..budget::MAX_DOCUMENT_DIAGRAMS).map(|index| format!("n{index}")).collect();
        for (index, source) in sources.iter().enumerate() {
            recent.keep(source, &MRK_DARK.palette, index);
        }

        assert!(sources.iter().enumerate().all(|(index, source)| recent.find(source, &MRK_DARK.palette) == Some(index)));
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
