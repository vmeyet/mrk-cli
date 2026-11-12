use std::io;
use std::time::Duration;

use crossterm::event::{self, Event};

/// Where the pager's events and the window size come from: the terminal, or a script in tests.
pub trait Events {
    fn read(&mut self) -> io::Result<Event>;
    /// Whether an event comes within `timeout`.
    fn poll(&mut self, timeout: Duration) -> io::Result<bool>;
    /// The window's columns and rows.
    fn size(&mut self) -> io::Result<(u16, u16)>;
}

/// The terminal's events. crossterm watches `SIGWINCH` only once its event reader started, so `listen` starts it: a
/// resize after it arrives as an event, and one before it shows in `size`.
pub struct Terminal(());

impl Terminal {
    pub fn listen() -> io::Result<Self> {
        event::poll(Duration::ZERO)?;
        Ok(Self(()))
    }
}

impl Events for Terminal {
    fn read(&mut self) -> io::Result<Event> {
        event::read()
    }

    fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        event::poll(timeout)
    }

    fn size(&mut self) -> io::Result<(u16, u16)> {
        crossterm::terminal::size()
    }
}

/// The last of a burst of resize events, so dragging the window edge renders once, not once per column, and the
/// first other event that ended the burst, still to handle.
pub fn latest_size(events: &mut impl Events, columns: u16, rows: u16) -> io::Result<((u16, u16), Option<Event>)> {
    let mut size = (columns, rows);
    while events.poll(Duration::ZERO)? {
        match events.read()? {
            Event::Resize(columns, rows) => size = (columns, rows),
            other => return Ok((size, Some(other))),
        }
    }
    Ok((size, None))
}
