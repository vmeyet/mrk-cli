use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use signal_hook::iterator::Signals;
use signal_hook::low_level;

/// The signals that end mrk by default, as a parent program closing it sends them: caught, so the pager gives the
/// terminal back before mrk ends.
const ENDING: [i32; 4] = [SIGHUP, SIGINT, SIGQUIT, SIGTERM];
/// How long the pager has to give the terminal back before another ending signal ends mrk at once; a parent often
/// sends two in a row (`SIGHUP` then `SIGTERM`), which must not skip that.
const PATIENCE: Duration = Duration::from_secs(1);

/// The first ending signal caught while the pager holds the terminal.
pub struct Ending {
    caught: Arc<AtomicUsize>,
}

impl Ending {
    pub fn watch() -> io::Result<Self> {
        let mut signals = Signals::new(ENDING)?;
        let caught = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&caught);
        thread::spawn(move || {
            let mut first = None;
            for signal in signals.forever() {
                match first {
                    None => {
                        first = Some(Instant::now());
                        seen.store(signal as usize, Ordering::SeqCst);
                    }
                    Some(at) if at.elapsed() >= PATIENCE => {
                        let _ = low_level::emulate_default_handler(signal);
                    }
                    Some(_) => {}
                }
            }
        });
        Ok(Self { caught })
    }

    /// The signal caught, if any.
    pub fn caught(&self) -> Option<i32> {
        match self.caught.load(Ordering::SeqCst) {
            0 => None,
            signal => Some(signal as i32),
        }
    }
}

/// Ends mrk as `signal` would have, once the terminal is given back.
pub fn end_as(signal: i32) -> io::Result<()> {
    low_level::emulate_default_handler(signal)
}
