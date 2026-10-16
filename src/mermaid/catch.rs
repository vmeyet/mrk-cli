use std::cell::Cell;
use std::panic::{self, AssertUnwindSafe};

use super::DiagramError;

thread_local! {
    static CATCHING: Cell<bool> = const { Cell::new(false) };
}

/// Runs a renderer with its panics caught. Its report quotes the diagram, which stderr would print unsanitized
/// (`specs/02-security.md` rule 1): a panic hook leaves it out while `is_catching` says so.
pub fn caught<T>(render: impl FnOnce() -> T) -> Result<T, DiagramError> {
    let outer = CATCHING.replace(true);
    let rendered = panic::catch_unwind(AssertUnwindSafe(render));
    CATCHING.set(outer);
    rendered.map_err(|_| DiagramError::Crashed)
}

/// Whether a panic on this thread now would be caught by the Mermaid renderers, which recover from it: a panic hook
/// leaves its report out, since that report quotes the diagram.
pub fn is_catching() -> bool {
    CATCHING.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_renderer_that_returns_is_passed_through() {
        assert_eq!(caught(|| 42), Ok(42));
    }

    #[test]
    fn a_panic_is_a_crash_and_only_counts_as_caught_while_running() {
        let during = caught(is_catching);

        assert_eq!(caught(|| panic!("boom")), Err::<(), _>(DiagramError::Crashed));
        assert_eq!(during, Ok(true));
        assert!(!is_catching());
    }

    #[test]
    fn a_nested_catch_leaves_the_outer_one_catching() {
        let inner = caught(|| {
            assert_eq!(caught(|| panic!("inner")), Err::<(), _>(DiagramError::Crashed));
            is_catching()
        });

        assert_eq!(inner, Ok(true));
        assert!(!is_catching());
    }
}
