use std::io;
use std::time::Duration;

use super::reply::{self, Replies};
use super::tmux;

const TIMEOUT: Duration = Duration::from_millis(100);
const GRAPHICS_QUERY: &str = "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";
const BACKGROUND_QUERY: &str = "\x1b]11;?\x1b\\";
const CELL_SIZE_QUERY: &str = "\x1b[16t";
const PRIMARY_ATTRIBUTES_QUERY: &str = "\x1b[c";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Questions {
    /// Whether kitty graphics work; the DA1 reply that ends every round tells about Sixel for free.
    pub graphics: bool,
    pub cell: bool,
    pub background: bool,
    /// Inside tmux: the graphics query goes through to the terminal outside, which answers after tmux's own DA1.
    pub passthrough: bool,
}

impl Questions {
    pub fn is_empty(self) -> bool {
        !self.graphics && !self.cell && !self.background
    }
}

fn request(questions: Questions) -> String {
    let graphics = match (questions.graphics, questions.passthrough) {
        (false, _) => String::new(),
        (true, true) => tmux::passthrough(GRAPHICS_QUERY),
        (true, false) => GRAPHICS_QUERY.to_owned(),
    };
    let cell = if questions.cell { CELL_SIZE_QUERY } else { "" };
    let background = if questions.background { BACKGROUND_QUERY } else { "" };
    format!("{graphics}{cell}{background}{PRIMARY_ATTRIBUTES_QUERY}")
}

#[cfg(unix)]
mod tty {
    use std::fs::{File, OpenOptions};
    use std::io::{self, Read, Write};
    use std::time::{Duration, Instant};

    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};

    use super::reply::{self, Replies};

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

    fn read_replies(tty: &File, deadline: Instant, is_complete: impl Fn(&Replies) -> bool) -> io::Result<Vec<u8>> {
        let mut received = Vec::new();
        while !is_complete(&reply::parse(&received)) && received.len() < MAX_REPLY_BYTES && Instant::now() < deadline {
            let chunk = read_available(tty)?;
            if chunk.is_empty() {
                std::thread::sleep(IDLE_WAIT);
            }
            received.extend(chunk);
        }
        Ok(received)
    }

    pub fn exchange(request: &str, timeout: Duration, is_complete: impl Fn(&Replies) -> bool) -> io::Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut tty = open_nonblocking()?;
        let _raw = RawMode::enable()?;
        tty.write_all(request.as_bytes())?;
        tty.flush()?;
        read_replies(&tty, deadline, is_complete)
    }
}

#[cfg(unix)]
fn exchange(request: &str, is_complete: impl Fn(&Replies) -> bool) -> io::Result<Vec<u8>> {
    tty::exchange(request, TIMEOUT, is_complete)
}

#[cfg(not(unix))]
fn exchange(_request: &str, _is_complete: impl Fn(&Replies) -> bool) -> io::Result<Vec<u8>> {
    Err(io::ErrorKind::Unsupported.into())
}

/// The round is over once DA1 is answered, and through tmux once the terminal outside said yes to graphics too: one
/// that does not speak kitty graphics never answers, so that round lasts until the timeout.
fn is_complete(replies: &Replies, questions: Questions) -> bool {
    let waits_outside = questions.passthrough && questions.graphics;
    replies.answered && (!waits_outside || replies.graphics)
}

/// Asks the terminal on `/dev/tty` in one round trip ended by a DA1 request, in raw mode, for at most 100 ms.
pub fn ask(questions: Questions) -> Replies {
    exchange(&request(questions), |replies| is_complete(replies, questions)).map(|bytes| reply::parse(&bytes)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_request_ends_with_the_da1_sentinel() {
        assert_eq!(request(Questions::default()), "\x1b[c");
        assert_eq!(
            request(Questions { graphics: true, cell: true, background: true, passthrough: false }),
            format!("{GRAPHICS_QUERY}\x1b[16t{BACKGROUND_QUERY}\x1b[c")
        );
        assert_eq!(request(Questions { background: true, ..Questions::default() }), "\x1b]11;?\x1b\\\x1b[c");
    }

    #[test]
    fn inside_tmux_only_the_graphics_query_passes_through() {
        let asked = request(Questions { graphics: true, cell: true, passthrough: true, ..Questions::default() });

        assert_eq!(asked, format!("{}\x1b[16t\x1b[c", tmux::passthrough(GRAPHICS_QUERY)));
    }

    #[test]
    fn through_tmux_the_round_waits_for_the_graphics_reply_after_da1() {
        let through_tmux = Questions { graphics: true, passthrough: true, ..Questions::default() };
        let tmux_only = Replies { answered: true, ..Replies::default() };
        let both = Replies { graphics: true, ..tmux_only };

        assert!(!is_complete(&tmux_only, through_tmux));
        assert!(is_complete(&both, through_tmux));
        assert!(is_complete(&tmux_only, Questions { graphics: true, ..Questions::default() }));
        assert!(is_complete(&tmux_only, Questions { passthrough: true, ..Questions::default() }));
    }
}
