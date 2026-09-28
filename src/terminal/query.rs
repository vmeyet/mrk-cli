use std::io;
use std::time::Duration;

use super::reply::{self, Replies};

const TIMEOUT: Duration = Duration::from_millis(100);
const GRAPHICS_QUERY: &str = "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";
const BACKGROUND_QUERY: &str = "\x1b]11;?\x1b\\";
const PRIMARY_ATTRIBUTES_QUERY: &str = "\x1b[c";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Questions {
    pub graphics: bool,
    pub background: bool,
}

impl Questions {
    pub fn is_empty(self) -> bool {
        !self.graphics && !self.background
    }
}

fn request(questions: Questions) -> String {
    let graphics = if questions.graphics { GRAPHICS_QUERY } else { "" };
    let background = if questions.background { BACKGROUND_QUERY } else { "" };
    format!("{graphics}{background}{PRIMARY_ATTRIBUTES_QUERY}")
}

#[cfg(unix)]
mod tty {
    use std::fs::{File, OpenOptions};
    use std::io::{self, Read, Write};
    use std::time::{Duration, Instant};

    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};

    use super::reply;

    const MAX_REPLY_BYTES: usize = 4096;
    const READ_BYTES: usize = 256;
    const IDLE_WAIT: Duration = Duration::from_millis(2);

    struct RawMode;

    impl RawMode {
        fn enable() -> io::Result<Self> {
            crossterm::terminal::enable_raw_mode()?;
            Ok(Self)
        }
    }

    impl Drop for RawMode {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }

    /// Non-blocking reads rather than `poll`, which macOS refuses on `/dev/tty`.
    fn open_nonblocking() -> io::Result<File> {
        let tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
        fcntl_setfl(&tty, fcntl_getfl(&tty)? | OFlags::NONBLOCK)?;
        Ok(tty)
    }

    fn read_available(mut tty: &File) -> io::Result<Vec<u8>> {
        let mut buffer = [0; READ_BYTES];
        match tty.read(&mut buffer) {
            Ok(count) => Ok(buffer[..count].to_vec()),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    fn read_replies(tty: &File, deadline: Instant) -> io::Result<Vec<u8>> {
        let mut received = Vec::new();
        while !reply::parse(&received).answered && received.len() < MAX_REPLY_BYTES && Instant::now() < deadline {
            let chunk = read_available(tty)?;
            if chunk.is_empty() {
                std::thread::sleep(IDLE_WAIT);
            }
            received.extend(chunk);
        }
        Ok(received)
    }

    pub fn exchange(request: &str, timeout: Duration) -> io::Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut tty = open_nonblocking()?;
        let _raw = RawMode::enable()?;
        tty.write_all(request.as_bytes())?;
        tty.flush()?;
        read_replies(&tty, deadline)
    }
}

#[cfg(unix)]
fn exchange(request: &str) -> io::Result<Vec<u8>> {
    tty::exchange(request, TIMEOUT)
}

#[cfg(not(unix))]
fn exchange(_request: &str) -> io::Result<Vec<u8>> {
    Err(io::ErrorKind::Unsupported.into())
}

/// Asks the terminal on `/dev/tty` in one round trip ended by a DA1 request, in raw mode, for at most 100 ms.
pub fn ask(questions: Questions) -> Replies {
    exchange(&request(questions)).map(|bytes| reply::parse(&bytes)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_request_ends_with_the_da1_sentinel() {
        assert_eq!(request(Questions::default()), "\x1b[c");
        assert_eq!(request(Questions { graphics: true, background: true }), format!("{GRAPHICS_QUERY}{BACKGROUND_QUERY}\x1b[c"));
        assert_eq!(request(Questions { graphics: false, background: true }), "\x1b]11;?\x1b\\\x1b[c");
    }
}
