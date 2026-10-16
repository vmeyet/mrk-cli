use std::io;
use std::thread;

use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use signal_hook::iterator::Signals;
use signal_hook::low_level;

/// The signals that end mrk by default, as a parent program closing it sends them.
const ENDING: [i32; 4] = [SIGHUP, SIGINT, SIGQUIT, SIGTERM];

/// From now on, an ending signal runs `give_back` then ends mrk as that signal would have. Both run on the signal
/// thread, which waits for nothing the pager does: one that arrives while a new width renders ends mrk at once.
pub fn on_ending(give_back: impl Fn() + Send + 'static) -> io::Result<()> {
    let mut signals = Signals::new(ENDING)?;
    thread::spawn(move || {
        for signal in signals.forever() {
            give_back();
            let _ = low_level::emulate_default_handler(signal);
        }
    });
    Ok(())
}
