use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsFd;

use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use rustix::termios::{OptionalActions, Termios, tcgetattr, tcsetattr};

/// The terminal mrk runs in, whatever its standard streams are.
pub fn open() -> io::Result<File> {
    OpenOptions::new().read(true).write(true).open("/dev/tty")
}

/// `open`, with reads and writes that never wait.
pub fn open_nonblocking() -> io::Result<File> {
    let tty = open()?;
    fcntl_setfl(&tty, fcntl_getfl(&tty)? | OFlags::NONBLOCK)?;
    Ok(tty)
}

/// A tty in raw mode until dropped. Restoring with `TCSAFLUSH` discards the input left unread in the same step, so the
/// shell never reads a terminal reply that came too late as keys.
pub struct RawMode<T: AsFd> {
    tty: T,
    saved: Termios,
}

impl<T: AsFd> RawMode<T> {
    pub fn enable(tty: T) -> io::Result<Self> {
        let saved = tcgetattr(&tty)?;
        let mut raw = saved.clone();
        raw.make_raw();
        tcsetattr(&tty, OptionalActions::Now, &raw)?;
        Ok(Self { tty, saved })
    }

    /// The modes the tty had before.
    pub fn saved(&self) -> &Termios {
        &self.saved
    }
}

impl<T: AsFd> Drop for RawMode<T> {
    fn drop(&mut self) {
        let _ = tcsetattr(&self.tty, OptionalActions::Flush, &self.saved);
    }
}
