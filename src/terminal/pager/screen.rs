use std::io::{self, Write};
use std::panic::{self, PanicHookInfo};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use super::frame;

/// Alternate screen, cursor hidden, no line wrap so a line never spills onto the next row, and the wheel sent as
/// arrow keys (alternate scroll) so it scrolls without capturing the mouse, which would break text selection.
const ENTER: &str = "\x1b[?1049h\x1b[?25l\x1b[?7l\x1b[?1007h";
const LEAVE: &str = "\x1b[?1007l\x1b[?2026l\x1b[?7h\x1b[?25h\x1b[?1049l";

type Hook = Box<dyn Fn(&PanicHookInfo<'_>) + Sync + Send>;

fn give_back() {
    let mut out = io::stdout();
    let _ = write!(out, "{}{LEAVE}", frame::forget_pictures()).and_then(|()| out.flush());
    let _ = crossterm::terminal::disable_raw_mode();
}

/// The report of the last panic on the pager's thread, kept instead of printed onto the alternate screen, where it
/// would be lost. A hook cannot tell a panic the mermaid renderers catch from one that ends mrk, so it never touches
/// the terminal: a caught panic leaves the pager running, an uncaught one unwinds through `Screen`'s `Drop`.
struct HeldPanics {
    report: Arc<Mutex<Option<String>>>,
    previous: Arc<Hook>,
}

impl HeldPanics {
    fn hold() -> Self {
        let previous: Arc<Hook> = Arc::new(panic::take_hook());
        let report = Arc::new(Mutex::new(None));
        let pager_thread = thread::current().id();
        let (hook_report, hook_previous) = (Arc::clone(&report), Arc::clone(&previous));
        panic::set_hook(Box::new(move |info| {
            if thread::current().id() == pager_thread {
                *hook_report.lock().unwrap_or_else(PoisonError::into_inner) = Some(info.to_string());
            } else {
                hook_previous(info);
            }
        }));
        Self { report, previous }
    }

    fn report(&self) -> Option<String> {
        self.report.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

/// Runs after `Screen` gave the terminal back. The hook cannot be swapped while unwinding (`set_hook` panics then),
/// and the process is ending anyway.
impl Drop for HeldPanics {
    fn drop(&mut self) {
        if !thread::panicking() {
            let previous = Arc::clone(&self.previous);
            panic::set_hook(Box::new(move |info| previous(info)));
        } else if let Some(report) = self.report() {
            eprintln!("{report}");
        }
    }
}

/// The terminal as the pager holds it; dropping it, also when a panic unwinds out of the pager, gives the terminal
/// back as it was.
pub struct Screen {
    _panics: HeldPanics,
}

impl Screen {
    pub fn take() -> io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        let screen = Self { _panics: HeldPanics::hold() };
        let mut out = io::stdout();
        out.write_all(ENTER.as_bytes())?;
        out.flush()?;
        Ok(screen)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        give_back();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caught_panic_is_only_recorded() {
        let panics = HeldPanics::hold();

        let caught = panic::catch_unwind(|| panic!("the diagram engine gave up"));

        assert!(caught.is_err());
        assert!(panics.report().is_some_and(|report| report.contains("the diagram engine gave up")));
    }
}
