use std::fs::File;
use std::io::{self, Stdout, Write};
use std::panic::{self, PanicHookInfo};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;

use super::frame::Pictures;
use super::page::Page;
use rustix::termios::{OptionalActions, Termios, tcsetattr};

use super::signals;
use crate::terminal::environment::{self, Environment};
use crate::terminal::tty::{self, RawMode};
use crate::{mermaid, sanitize};

/// Alternate screen, cursor hidden, and no line wrap so a line never spills onto the next row.
const ENTER: &str = "\x1b[?1049h\x1b[?25l\x1b[?7l";
const LEAVE: &str = "\x1b[?2026l\x1b[?7h\x1b[?25h\x1b[?1049l";
/// The wheel sent as arrow keys (alternate scroll), so it scrolls without capturing the mouse, which would break text
/// selection; saved first (XTSAVE), so leaving restores it (XTRESTORE).
const SAVE_ALTERNATE_SCROLL: &str = "\x1b[?1007s\x1b[?1007h";
const RESTORE_ALTERNATE_SCROLL: &str = "\x1b[?1007r";

type Hook = Box<dyn Fn(&PanicHookInfo<'_>) + Sync + Send>;

/// What takes the terminal's modes, and what gives them back: alternate scroll only where the terminal restores it.
fn modes(saves_modes: bool) -> (String, String) {
    if saves_modes {
        (format!("{ENTER}{SAVE_ALTERNATE_SCROLL}"), format!("{RESTORE_ALTERNATE_SCROLL}{LEAVE}"))
    } else {
        (ENTER.to_owned(), LEAVE.to_owned())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The terminal as the pager holds it.
struct Taken {
    out: Stdout,
    raw: RawMode<File>,
    leave: String,
    /// What frees the pictures last stored.
    free: String,
}

impl Taken {
    /// The terminal taken, and how to force it back.
    fn enter() -> io::Result<(Self, Forced)> {
        let raw = RawMode::enable(tty::open()?)?;
        let (enter, leave) = modes(environment::saves_modes(&Environment::read()));
        let forced = Forced { tty: tty::open_nonblocking()?, saved: raw.saved().clone(), leave: format!("{STRING_END}{leave}") };
        let mut out = io::stdout();
        out.write_all(enter.as_bytes())?;
        out.flush()?;
        Ok((Self { out, raw, leave, free: String::new() }, forced))
    }

    /// Frees the pictures, leaves the pager's modes, then restores the tty's, which waits for the output to drain.
    fn give_back(mut self) {
        let _ = self.out.write_all((self.free + &self.leave).as_bytes()).and_then(|()| self.out.flush());
        drop(self.raw);
    }
}

/// Ends a string sequence (APC, DCS) a stalled write may have left open, so the bytes after it are read as sequences.
const STRING_END: &str = "\x1b\\";

/// The terminal given back without waiting on anything: the tty opened again without blocking, so a terminal that
/// stopped reading drops the leave sequence rather than hold mrk, and its modes restored at once.
struct Forced {
    tty: File,
    saved: Termios,
    leave: String,
}

impl Forced {
    fn give_back(&self) {
        let _ = rustix::io::write(&self.tty, self.leave.as_bytes());
        let _ = tcsetattr(&self.tty, OptionalActions::Now, &self.saved);
    }
}

/// What the pager thread and the signal thread share. Every write to the terminal holds `taken`, so the terminal is
/// given back between two writes, once, by whichever thread comes first.
#[derive(Default)]
struct Held {
    taken: Mutex<Option<Taken>>,
    forced: OnceLock<Forced>,
    /// The reports of the panics while the screen was held, printed once the terminal is back: on the alternate
    /// screen they would be lost.
    reports: Mutex<Vec<String>>,
}

impl Held {
    fn hold(&self, report: String) {
        lock(&self.reports).push(report);
    }

    /// Holds `taken` until the terminal is back, so a signal meanwhile waits rather than end mrk halfway.
    fn give_back(&self) {
        let mut taken = lock(&self.taken);
        let Some(given) = taken.take() else { return };
        given.give_back();
        drop(taken);
        let _ = io::stderr().write_all(printed(&std::mem::take(&mut *lock(&self.reports))).as_bytes());
    }

    fn force_back(&self) {
        if let Some(forced) = self.forced.get() {
            forced.give_back();
        }
    }
}

fn printed(reports: &[String]) -> String {
    reports.iter().map(|report| sanitize::lines(report) + "\n").collect()
}

fn report(info: &PanicHookInfo<'_>) -> String {
    format!("thread '{}' {info}", thread::current().name().unwrap_or("<unnamed>"))
}

/// Keeps every panic report until the terminal is back, but for a panic the mermaid renderers catch, which leaves the
/// pager running and whose report quotes the diagram.
fn hold_panics(held: &Arc<Held>) -> Hook {
    let previous = panic::take_hook();
    let held = Arc::clone(held);
    panic::set_hook(Box::new(move |info| {
        if !mermaid::is_catching() {
            held.hold(report(info));
        }
    }));
    previous
}

/// The terminal as the pager holds it; dropping it, also when a panic unwinds out of the pager, frees the pictures it
/// stored and gives the terminal back as it was. An ending signal does the same on its own thread, then ends mrk.
pub struct Screen {
    held: Arc<Held>,
    previous_hook: Option<Hook>,
}

impl Screen {
    /// Signals are caught first, and the terminal changes while `taken` is held, so a signal from then on waits for it
    /// and gives back whatever was taken.
    pub fn take() -> io::Result<Self> {
        let held = Arc::new(Held::default());
        let (giver, forcer) = (Arc::clone(&held), Arc::clone(&held));
        signals::on_ending(move || giver.give_back(), move || forcer.force_back())?;
        let mut taken = lock(&held.taken);
        let (entered, forced) = Taken::enter()?;
        *taken = Some(entered);
        let _ = held.forced.set(forced);
        drop(taken);
        Ok(Self { previous_hook: Some(hold_panics(&held)), held })
    }

    /// Sends the page's pictures once, after freeing the ones stored for the page before.
    pub fn store(&self, pictures: &Pictures, page: &Page) -> io::Result<()> {
        let (free, stored) = (pictures.free(page), pictures.store(page));
        let mut taken = lock(&self.held.taken);
        let Some(taken) = taken.as_mut() else { return Ok(()) };
        let previous = std::mem::replace(&mut taken.free, free);
        taken.out.write_all((previous + &stored).as_bytes())
    }

    pub fn show(&self, frame: &str) -> io::Result<()> {
        let mut taken = lock(&self.held.taken);
        let Some(taken) = taken.as_mut() else { return Ok(()) };
        taken.out.write_all(frame.as_bytes())?;
        taken.out.flush()
    }
}

/// The hook cannot be swapped back while unwinding (`set_hook` panics then), and the process is ending anyway.
impl Drop for Screen {
    fn drop(&mut self) {
        self.held.give_back();
        if let Some(previous) = self.previous_hook.take().filter(|_| !thread::panicking()) {
            panic::set_hook(previous);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alternate_scroll_is_saved_and_restored_only_where_the_terminal_restores_modes() {
        assert_eq!(modes(true), (format!("{ENTER}\x1b[?1007s\x1b[?1007h"), format!("\x1b[?1007r{LEAVE}")));
        assert_eq!(modes(false), (ENTER.to_owned(), LEAVE.to_owned()));
        assert!(!modes(false).1.contains("1007"));
    }

    #[test]
    fn held_reports_are_printed_sanitized_one_after_the_other() {
        let held = Held::default();
        held.hold("thread 'main' panicked at a.rs:1:1:\n\x1b]52;c;aGk=\x07boom".to_owned());
        held.hold("thread 'worker' panicked at b.rs:2:2:\nbust".to_owned());

        assert_eq!(
            printed(&lock(&held.reports)),
            "thread 'main' panicked at a.rs:1:1:\n]52;c;aGk=boom\nthread 'worker' panicked at b.rs:2:2:\nbust\n"
        );
    }
}
