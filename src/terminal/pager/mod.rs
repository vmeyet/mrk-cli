mod frame;
mod keys;
mod page;
mod picture;
mod screen;
mod search;
mod signals;
mod state;
mod status;

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};

use self::frame::{Look, Pictures};
use self::keys::Keys;
use self::page::Page;
use self::screen::Screen;
use self::state::Pager;
use super::Capabilities;
use crate::document::Document;
use crate::theme::Palette;

pub use self::keys::help as key_help;

/// The document laid out for a window this many columns wide, and the margin that centres it.
pub struct Rendered {
    pub document: Document,
    pub margin: usize,
}

/// What stays the same for the whole session.
pub struct Session<'a> {
    /// The file name, or `stdin`, for the status bar.
    pub name: &'a str,
    pub palette: &'a Palette,
    pub capabilities: &'a Capabilities,
}

enum Step {
    Act(keys::Action),
    Resize(u16, u16),
    Quit,
    Nothing,
}

struct View {
    pager: Pager,
    margin: usize,
    columns: u16,
    pictures: Pictures,
}

fn content_height(rows: u16) -> usize {
    usize::from(rows.saturating_sub(1)).max(1)
}

/// What the event does, and the keys after it.
fn step(keys: Keys, event: &Event, is_prompting: bool) -> (Keys, Step) {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            let (keys, action) = keys.press(*key, is_prompting);
            let step = match action {
                Some(keys::Action::Quit) => Step::Quit,
                Some(action) => Step::Act(action),
                None => Step::Nothing,
            };
            (keys, step)
        }
        Event::Resize(columns, rows) => (keys, Step::Resize(*columns, *rows)),
        _ => (keys, Step::Nothing),
    }
}

/// The last of a burst of resize events, so dragging the window edge renders once, not once per column, and the
/// first other event that ended the burst, still to handle.
fn latest_size(columns: u16, rows: u16) -> io::Result<((u16, u16), Option<Event>)> {
    let mut size = (columns, rows);
    while event::poll(Duration::ZERO)? {
        match event::read()? {
            Event::Resize(columns, rows) => size = (columns, rows),
            other => return Ok((size, Some(other))),
        }
    }
    Ok((size, None))
}

/// A terminal reply arrives in one burst; a pause this long inside one means the keys are the reader's.
const REPLY_GAP: Duration = Duration::from_millis(20);

struct Reader<'a, R: Fn(u16) -> Rendered> {
    session: &'a Session<'a>,
    render: R,
}

impl<R: Fn(u16) -> Rendered> Reader<'_, R> {
    fn pictures(&self, page: &Page) -> Pictures {
        Pictures::new(page, self.session.capabilities, std::process::id())
    }

    fn first_view(&self, (columns, rows): (u16, u16)) -> View {
        let Rendered { document, margin } = (self.render)(columns);
        let pager = Pager::new(page::flatten(document), content_height(rows));
        View { pictures: self.pictures(&pager.page), pager, margin, columns }
    }

    /// Draws the view, which comes back with the Sixel bands it sent kept for the next frame.
    fn draw(&self, screen: &Screen, view: View) -> io::Result<View> {
        let view = View { pictures: view.pictures.encoded(&view.pager), ..view };
        let capabilities = Capabilities { columns: view.columns, ..*self.session.capabilities };
        let session = self.session;
        let look = Look {
            name: session.name,
            palette: session.palette,
            capabilities: &capabilities,
            margin: view.margin,
            columns: usize::from(view.columns),
        };
        screen.show(&frame::frame(&view.pager, &look, &view.pictures))?;
        Ok(view)
    }

    /// Renders again only when the width changed; a new height just shows more or fewer rows.
    fn resized(&self, screen: &Screen, view: View, (columns, rows): (u16, u16)) -> io::Result<View> {
        if columns == view.columns {
            return Ok(View { pager: view.pager.with_height(content_height(rows)), ..view });
        }

        let Rendered { document, margin } = (self.render)(columns);
        let pager = view.pager.resized(page::flatten(document), content_height(rows));
        let view = View { pictures: self.pictures(&pager.page), pager, margin, columns };
        screen.store(&view.pictures, &view.pager.page)?;
        Ok(view)
    }
}

/// Shows the document on the alternate screen until the reader quits, re-rendering it with `render` whenever the
/// window changes width. Stays open even when the document fits, like a program another program hands the terminal to.
/// An ending signal gives the terminal back, then ends mrk as that signal would have.
pub fn run(session: &Session, render: impl Fn(u16) -> Rendered) -> io::Result<()> {
    let reader = Reader { session, render };
    let mut view = reader.first_view(crossterm::terminal::size()?);

    let screen = Screen::take()?;
    screen.store(&view.pictures, &view.pager.page)?;
    view = reader.draw(&screen, view)?;
    let (mut keys, mut pending) = (Keys::default(), None);
    loop {
        let event = match pending.take() {
            Some(event) => event,
            None => event::read()?,
        };
        let (next_keys, step) = step(keys, &event, view.pager.prompt.is_some());
        keys = if next_keys.is_reading_reply() && !event::poll(REPLY_GAP)? { Keys::default() } else { next_keys };
        match step {
            Step::Quit => return Ok(()),
            Step::Nothing => continue,
            Step::Act(action) => view = View { pager: view.pager.apply(action), ..view },
            Step::Resize(columns, rows) => {
                let (size, next) = latest_size(columns, rows)?;
                pending = next;
                view = reader.resized(&screen, view, size)?;
            }
        }
        view = reader.draw(&screen, view)?;
    }
}
