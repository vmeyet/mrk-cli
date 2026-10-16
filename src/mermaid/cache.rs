use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use super::DiagramError;
use crate::raster::Svg;
use crate::theme::Palette;

/// Enough for every diagram of a long document, small enough that a program rendering many never grows without end.
const CAPACITY: usize = 32;

type Drawing = Result<Arc<Svg>, DiagramError>;

static RECENT: Mutex<Recent<Drawing>> = Mutex::new(Recent::new());

/// The diagram of `source` in `palette`, parsed and laid out at most once while it stays among the recent ones: the
/// pager renders the document again on every resize, and only fitting and rasterising depend on the width.
pub fn drawing(source: &str, palette: &Palette, draw: impl FnOnce() -> Result<Svg, DiagramError>) -> Drawing {
    let known = RECENT.lock().unwrap_or_else(PoisonError::into_inner).find(source, palette);
    known.unwrap_or_else(|| {
        let drawing = draw().map(Arc::new);
        RECENT.lock().unwrap_or_else(PoisonError::into_inner).keep(source, palette, drawing.clone());
        drawing
    })
}

/// The last `CAPACITY` values by source and palette, oldest first.
struct Recent<T>(VecDeque<(String, Palette, T)>);

impl<T: Clone> Recent<T> {
    const fn new() -> Self {
        Self(VecDeque::new())
    }

    fn find(&self, source: &str, palette: &Palette) -> Option<T> {
        self.0.iter().find(|(known, known_palette, _)| known == source && known_palette == palette).map(|(_, _, value)| value.clone())
    }

    fn keep(&mut self, source: &str, palette: &Palette, value: T) {
        if self.0.len() == CAPACITY {
            self.0.pop_front();
        }
        self.0.push_back((source.to_owned(), *palette, value));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::theme::{MRK_DARK, MRK_LIGHT};

    fn square(side: u32) -> Result<Svg, DiagramError> {
        Ok(Svg::parse(&format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{side}" height="{side}"/>"#))?)
    }

    #[test]
    fn a_source_is_drawn_once_per_palette() {
        let source = "graph TD\n  cache --> test";
        let first = drawing(source, &MRK_DARK.palette, || square(10)).unwrap();
        let again = drawing(source, &MRK_DARK.palette, || unreachable!("drawn again")).unwrap();
        let light = drawing(source, &MRK_LIGHT.palette, || square(20)).unwrap();

        assert!(Arc::ptr_eq(&first, &again));
        assert_eq!((first.size(), light.size()), ((10.0, 10.0), (20.0, 20.0)));
    }

    #[test]
    fn a_failure_is_kept_too() {
        let source = "graph TD\n  cache --> failure";
        assert!(drawing(source, &MRK_DARK.palette, || Err(DiagramError::Crashed)).is_err());

        assert_eq!(drawing(source, &MRK_DARK.palette, || unreachable!("drawn again")).err(), Some(DiagramError::Crashed));
    }

    #[test]
    fn the_oldest_goes_first_when_full() {
        let mut recent = Recent::new();
        let sources: Vec<String> = (0..=CAPACITY).map(|index| format!("graph TD\n  n{index}")).collect();
        for (index, source) in sources.iter().enumerate() {
            recent.keep(source, &MRK_DARK.palette, index);
        }

        assert_eq!(recent.0.len(), CAPACITY);
        assert_eq!(recent.find(&sources[0], &MRK_DARK.palette), None);
        assert_eq!(recent.find(&sources[CAPACITY], &MRK_DARK.palette), Some(CAPACITY));
    }
}
