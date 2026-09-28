use std::io::{self, Write};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use super::frame;

/// Alternate screen, cursor hidden, no line wrap so a line never spills onto the next row.
const ENTER: &str = "\x1b[?1049h\x1b[?25l\x1b[?7l";
const LEAVE: &str = "\x1b[?2026l\x1b[?7h\x1b[?25h\x1b[?1049l";

static PANIC_HOOK: Once = Once::new();
static IS_HELD: AtomicBool = AtomicBool::new(false);

fn give_back() {
    if !IS_HELD.swap(false, Ordering::SeqCst) {
        return;
    }
    let mut out = io::stdout();
    let _ = write!(out, "{}{LEAVE}", frame::forget_pictures()).and_then(|()| out.flush());
    let _ = crossterm::terminal::disable_raw_mode();
}

/// Release builds abort on panic, so no destructor runs: the hook is what gives the terminal back then.
fn give_back_on_panic() {
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            give_back();
            previous(info);
        }));
    });
}

/// The terminal as the pager holds it; dropping it gives the terminal back as it was.
pub struct Screen;

impl Screen {
    pub fn take() -> io::Result<Self> {
        give_back_on_panic();
        crossterm::terminal::enable_raw_mode()?;
        IS_HELD.store(true, Ordering::SeqCst);
        let screen = Self;
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
