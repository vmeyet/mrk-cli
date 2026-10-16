use std::cell::Cell;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Once;

use super::DiagramError;

thread_local! {
    static CATCHING: Cell<bool> = const { Cell::new(false) };
}

static QUIET_HOOK: Once = Once::new();

/// Runs a renderer with its panics caught and their reports dropped: a report quotes the diagram, which stderr would
/// print unsanitized (`specs/02-security.md` rule 1).
pub fn caught<T>(render: impl FnOnce() -> T) -> Result<T, DiagramError> {
    QUIET_HOOK.call_once(quiet_hook);
    let outer = CATCHING.replace(true);
    let rendered = panic::catch_unwind(AssertUnwindSafe(render));
    CATCHING.set(outer);
    rendered.map_err(|_| DiagramError::Crashed)
}

/// Whether a panic on this thread now would be caught by `caught`, so a hook can leave its report out.
pub fn is_catching() -> bool {
    CATCHING.get()
}

/// Wraps the hook in place, so every panic `caught` does not catch still reaches it.
fn quiet_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if !is_catching() {
            previous(info);
        }
    }));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    use super::*;

    #[test]
    fn a_caught_panic_reports_nothing_and_any_other_still_reports() {
        let this_thread = thread::current().id();
        let reports = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&reports);
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if thread::current().id() == this_thread {
                counted.fetch_add(1, Ordering::SeqCst);
            }
            previous(info);
        }));
        quiet_hook();

        assert_eq!(caught(|| panic!("\u{1b}]52;c;aGk=\u{7}")), Err::<(), _>(DiagramError::Crashed));
        assert_eq!(reports.load(Ordering::SeqCst), 0);

        assert!(panic::catch_unwind(|| panic!("not caught by mermaid")).is_err());
        assert_eq!(reports.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_renderer_that_returns_is_passed_through() {
        assert_eq!(caught(|| 42), Ok(42));
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
