use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[cfg(unix)]
use signal_hook::consts::{SIGHUP, SIGQUIT};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::{flag, low_level};

/// The signals that end mrk by default, as a parent program closing it sends them: caught, so the pager gives the
/// terminal back before mrk ends.
#[cfg(unix)]
const ENDING: &[i32] = &[SIGHUP, SIGINT, SIGQUIT, SIGTERM];
#[cfg(not(unix))]
const ENDING: &[i32] = &[SIGINT, SIGTERM];

/// The first ending signal caught while the pager runs; a second one ends mrk at once, should the pager not answer.
pub struct Ending {
    caught: Arc<AtomicUsize>,
}

impl Ending {
    pub fn watch() -> io::Result<Self> {
        let caught = Arc::new(AtomicUsize::new(0));
        let is_caught = Arc::new(AtomicBool::new(false));
        for &signal in ENDING {
            flag::register_conditional_default(signal, Arc::clone(&is_caught))?;
            flag::register(signal, Arc::clone(&is_caught))?;
            flag::register_usize(signal, Arc::clone(&caught), signal as usize)?;
        }
        Ok(Self { caught })
    }

    pub fn is_caught(&self) -> bool {
        self.caught.load(Ordering::SeqCst) != 0
    }

    /// Ends mrk as the caught signal would have, once the terminal is given back; nothing when none was caught.
    pub fn finish(&self) -> io::Result<()> {
        match self.caught.load(Ordering::SeqCst) {
            0 => Ok(()),
            signal => low_level::emulate_default_handler(signal as i32),
        }
    }
}
