mod frame;
mod keys;
mod page;
mod picture;
mod screen;
mod search;
mod signals;
mod state;
mod status;

use std::io::{self, Write};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};

use self::frame::{Look, Pictures};
use self::page::Page;
use self::screen::Screen;
use self::signals::Ending;
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

fn step(event: &Event, is_prompting: bool) -> Step {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => match keys::action(*key, is_prompting) {
            Some(keys::Action::Quit) => Step::Quit,
            Some(action) => Step::Act(action),
            None => Step::Nothing,
        },
        Event::Resize(columns, rows) => Step::Resize(*columns, *rows),
        _ => Step::Nothing,
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

struct Reader<'a, R: Fn(u16) -> Rendered> {
    session: &'a Session<'a>,
    render: R,
}

impl<R: Fn(u16) -> Rendered> Reader<'_, R> {
    fn pictures(&self, page: &Page) -> Pictures {
        Pictures::new(page, self.session.capabilities.graphics.map(|graphics| graphics.protocol))
    }

    fn first_view(&self, (columns, rows): (u16, u16)) -> View {
        let Rendered { document, margin } = (self.render)(columns);
        let pager = Pager::new(page::flatten(document), content_height(rows));
        View { pictures: self.pictures(&pager.page), pager, margin, columns }
    }

    /// Draws the view, which comes back with the Sixel bands it sent kept for the next frame.
    fn draw(&self, out: &mut impl Write, view: View) -> io::Result<View> {
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
        out.write_all(frame::frame(&view.pager, &look, &view.pictures).as_bytes())?;
        out.flush()?;
        Ok(view)
    }

    /// Renders again only when the width changed; a new height just shows more or fewer rows.
    fn resized(&self, out: &mut impl Write, screen: &mut Screen, view: View, (columns, rows): (u16, u16)) -> io::Result<View> {
        if columns == view.columns {
            return Ok(View { pager: view.pager.with_height(content_height(rows)), ..view });
        }

        let Rendered { document, margin } = (self.render)(columns);
        let pager = view.pager.resized(page::flatten(document), content_height(rows));
        let view = View { pictures: self.pictures(&pager.page), pager, margin, columns };
        screen.store(out, &view.pictures, &view.pager.page)?;
        Ok(view)
    }
}

/// How often waiting for a key looks for an ending signal, which does not wake that wait.
const SIGNAL_CHECK: Duration = Duration::from_millis(100);

/// The event still to handle, else the next one from the terminal; `None` once an ending signal came.
fn next_event(pending: Option<Event>, ending: &Ending) -> io::Result<Option<Event>> {
    if pending.is_some() {
        return Ok(pending);
    }
    while ending.caught().is_none() {
        if event::poll(SIGNAL_CHECK)? {
            return event::read().map(Some);
        }
    }
    Ok(None)
}

/// Shows the document on the alternate screen until the reader quits, re-rendering it with `render` whenever the
/// window changes width. Stays open even when the document fits, like a program another program hands the terminal to.
/// An ending signal gives the terminal back, then ends mrk as that signal would have.
pub fn run(session: &Session, render: impl Fn(u16) -> Rendered) -> io::Result<()> {
    match show(&Reader { session, render })? {
        Some(signal) => signals::end_as(signal),
        None => Ok(()),
    }
}

/// The pager until the reader quits, or the ending signal that stopped it. Signals are caught only once the screen is
/// taken, so one during the first render ends mrk at once, with nothing to give back.
fn show(reader: &Reader<'_, impl Fn(u16) -> Rendered>) -> io::Result<Option<i32>> {
    let mut view = reader.first_view(crossterm::terminal::size()?);

    let mut screen = Screen::take()?;
    let ending = Ending::watch()?;
    let mut out = io::stdout().lock();
    screen.store(&mut out, &view.pictures, &view.pager.page)?;
    view = reader.draw(&mut out, view)?;
    let mut pending = None;
    loop {
        let Some(event) = next_event(pending.take(), &ending)? else { return Ok(ending.caught()) };
        match step(&event, view.pager.prompt.is_some()) {
            Step::Quit => return Ok(None),
            Step::Nothing => continue,
            Step::Act(action) => view = View { pager: view.pager.apply(action), ..view },
            Step::Resize(columns, rows) => {
                let (size, next) = latest_size(columns, rows)?;
                pending = next;
                view = reader.resized(&mut out, &mut screen, view, size)?;
            }
        }
        view = reader.draw(&mut out, view)?;
    }
}
