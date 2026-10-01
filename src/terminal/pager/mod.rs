mod frame;
mod keys;
mod page;
mod picture;
mod screen;
mod search;
mod state;
mod status;

use std::io::{self, Write};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};

use self::frame::{Look, Pictures};
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

/// Sends kitty pictures once per render, so frames only place them; Sixel pictures go out with every frame.
fn store(out: &mut impl Write, view: &View) -> io::Result<()> {
    if matches!(view.pictures, Pictures::Kitty) {
        out.write_all(frame::forget_pictures().as_bytes())?;
        out.write_all(frame::store_pictures(&view.pager.page).as_bytes())?;
    }
    Ok(())
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

    fn draw(&self, out: &mut impl Write, view: &View) -> io::Result<()> {
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
        out.flush()
    }

    /// Renders again only when the width changed; a new height just shows more or fewer rows.
    fn resized(&self, out: &mut impl Write, view: View, (columns, rows): (u16, u16)) -> io::Result<View> {
        if columns == view.columns {
            return Ok(View { pager: view.pager.with_height(content_height(rows)), ..view });
        }

        let Rendered { document, margin } = (self.render)(columns);
        let pager = view.pager.resized(page::flatten(document), content_height(rows));
        let view = View { pictures: self.pictures(&pager.page), pager, margin, columns };
        store(out, &view)?;
        Ok(view)
    }
}

/// Shows the document on the alternate screen until the reader quits, re-rendering it with `render` whenever the
/// window changes width. Stays open even when the document fits, like a program another program hands the terminal to.
pub fn run(session: &Session, render: impl Fn(u16) -> Rendered) -> io::Result<()> {
    let reader = Reader { session, render };
    let mut view = reader.first_view(crossterm::terminal::size()?);

    let _screen = Screen::take()?;
    let mut out = io::stdout().lock();
    store(&mut out, &view)?;
    reader.draw(&mut out, &view)?;
    let mut pending = None;
    loop {
        let event = if let Some(event) = pending.take() { event } else { event::read()? };
        match step(&event, view.pager.prompt.is_some()) {
            Step::Quit => return Ok(()),
            Step::Nothing => continue,
            Step::Act(action) => view = View { pager: view.pager.apply(&action), ..view },
            Step::Resize(columns, rows) => {
                let (size, next) = latest_size(columns, rows)?;
                pending = next;
                view = reader.resized(&mut out, view, size)?;
            }
        }
        reader.draw(&mut out, &view)?;
    }
}
