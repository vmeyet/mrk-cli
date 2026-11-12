mod events;
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

use crossterm::event::{Event, KeyEventKind};

use self::events::{Events, Terminal};
use self::frame::{Look, Pictures};
use self::keys::Keys;
use self::page::Page;
use self::screen::{Canvas, Screen};
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

    /// The view at this window size, rendered again only when the width changed; a new height just shows more or fewer
    /// rows.
    fn fitted(&self, view: View, (columns, rows): (u16, u16)) -> View {
        if columns == view.columns {
            return View { pager: view.pager.with_height(content_height(rows)), ..view };
        }
        let Rendered { document, margin } = (self.render)(columns);
        let pager = view.pager.resized(page::flatten(document), content_height(rows));
        View { pictures: self.pictures(&pager.page), pager, margin, columns }
    }

    /// `fitted`, with the pictures of a new render stored before any frame shows them.
    fn resized(&self, canvas: &mut impl Canvas, view: View, size: (u16, u16)) -> io::Result<View> {
        let renders = size.0 != view.columns;
        let view = self.fitted(view, size);
        if renders {
            canvas.store(&view.pictures, &view.pager.page)?;
        }
        Ok(view)
    }

    /// Draws the view, which comes back with the Sixel bands it sent kept for the next frame.
    fn draw(&self, canvas: &mut impl Canvas, view: View) -> io::Result<View> {
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
        canvas.write_all(frame::frame(&view.pager, &look, &view.pictures).as_bytes())?;
        canvas.flush()?;
        Ok(view)
    }

    /// Shows `view`, laid out before the events were watched, then redraws after each event until the reader quits.
    fn read(&self, view: View, events: &mut impl Events, canvas: &mut impl Canvas) -> io::Result<()> {
        let view = self.fitted(view, events.size()?);
        canvas.store(&view.pictures, &view.pager.page)?;
        let mut view = self.draw(canvas, view)?;
        let (mut keys, mut pending) = (Keys::default(), None);
        loop {
            let event = match pending.take() {
                Some(event) => event,
                None => events.read()?,
            };
            let (next_keys, step) = step(keys, &event, view.pager.prompt.is_some());
            keys = if next_keys.is_reading_reply() && !events.poll(REPLY_GAP)? { Keys::default() } else { next_keys };
            match step {
                Step::Quit => return Ok(()),
                Step::Nothing => continue,
                Step::Act(action) => view = View { pager: view.pager.apply(action), ..view },
                Step::Resize(columns, rows) => {
                    let (size, next) = events::latest_size(events, columns, rows)?;
                    pending = next;
                    view = self.resized(canvas, view, size)?;
                }
            }
            view = self.draw(canvas, view)?;
        }
    }
}

/// Shows the document on the alternate screen until the reader quits, re-rendering it with `render` whenever the
/// window changes width. Stays open even when the document fits, like a program another program hands the terminal to.
/// An ending signal gives the terminal back, then ends mrk as that signal would have.
pub fn run(session: &Session, render: impl Fn(u16) -> Rendered) -> io::Result<()> {
    let reader = Reader { session, render };
    let view = reader.first_view(crossterm::terminal::size()?);
    let mut screen = Screen::take()?;
    reader.read(view, &mut Terminal::listen()?, &mut screen)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::io::Write;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::frame::BEGIN_SYNCHRONIZED;
    use super::*;
    use crate::document::{Block, CellSize, Line, Picture, Span};
    use crate::terminal::{ColorDepth, Graphics, Protocol};
    use crate::theme::MRK_DARK;

    const FIRST_SIZE: (u16, u16) = (40, 10);

    const TEXT_ONLY: Capabilities = Capabilities {
        color: ColorDepth::TrueColor,
        hyperlinks: true,
        graphics: None,
        double_height: false,
        background: None,
        columns: 40,
        rows: 10,
        is_terminal: true,
    };

    enum Scripted {
        Event(Event),
        /// No event for longer than any wait of the pager.
        Pause,
    }

    /// Events as the script lists them, then an error, so a pager that does not quit fails instead of waiting.
    struct Script {
        size: (u16, u16),
        steps: VecDeque<Scripted>,
    }

    impl Events for Script {
        fn read(&mut self) -> io::Result<Event> {
            loop {
                match self.steps.pop_front() {
                    Some(Scripted::Event(event)) => return Ok(event),
                    Some(Scripted::Pause) => {}
                    None => return Err(io::ErrorKind::UnexpectedEof.into()),
                }
            }
        }

        fn poll(&mut self, _: Duration) -> io::Result<bool> {
            match self.steps.front() {
                Some(Scripted::Event(_)) => Ok(true),
                Some(Scripted::Pause) => Ok(self.steps.pop_front().is_none()),
                None => Ok(false),
            }
        }

        fn size(&mut self) -> io::Result<(u16, u16)> {
            Ok(self.size)
        }
    }

    impl Canvas for Vec<u8> {
        fn store(&mut self, pictures: &Pictures, page: &Page) -> io::Result<()> {
            self.write_all(pictures.store(page).as_bytes())
        }
    }

    fn key(code: KeyCode, modifiers: KeyModifiers) -> Scripted {
        Scripted::Event(Event::Key(KeyEvent::new(code, modifiers)))
    }

    fn press(character: char) -> Scripted {
        key(KeyCode::Char(character), KeyModifiers::NONE)
    }

    fn resize(columns: u16, rows: u16) -> Scripted {
        Scripted::Event(Event::Resize(columns, rows))
    }

    /// Thirty rows, `row 00` to `row 29`, then a picture.
    fn document() -> Document {
        let rows = (0..30).map(|row| Line::new(vec![Span::plain(format!("row {row:02}"))])).collect();
        let png = crate::terminal::sixel::test_png(20, 20, &[[255, 0, 0, 255]; 400]);
        let picture = Picture { png, cols: 2, rows: 1, alt: "dot".to_owned(), indent: Line::blank(), concealed_text: None };
        Document { blocks: vec![Block::Lines(rows), Block::Picture(picture)] }
    }

    /// What the pager wrote, and the widths it rendered at, laid out at `FIRST_SIZE`, then reading `steps` in a window
    /// `size` big.
    fn paged(capabilities: &Capabilities, size: (u16, u16), steps: Vec<Scripted>) -> (String, Vec<u16>) {
        let widths = RefCell::new(Vec::new());
        let render = |columns| {
            widths.borrow_mut().push(columns);
            Rendered { document: document(), margin: 0 }
        };
        let session = Session { name: "a.md", palette: &MRK_DARK.palette, capabilities };
        let reader = Reader { session: &session, render };
        let mut written = Vec::new();

        let read = reader.read(reader.first_view(FIRST_SIZE), &mut Script { size, steps: steps.into() }, &mut written);

        assert!(read.is_ok(), "{read:?}");
        (String::from_utf8_lossy(&written).into_owned(), widths.into_inner())
    }

    fn frames(written: &str) -> Vec<&str> {
        written.split(BEGIN_SYNCHRONIZED).skip(1).collect()
    }

    #[test]
    fn a_burst_of_resizes_renders_once_at_the_last_size_then_handles_the_key_that_ended_it() {
        let (written, widths) = paged(&TEXT_ONLY, FIRST_SIZE, vec![resize(50, 10), resize(60, 10), resize(70, 12), press('j'), press('q')]);
        let frames = frames(&written);

        assert_eq!(widths, [40, 70]);
        assert_eq!(frames.len(), 3);
        assert!(frames[1].contains("\x1b[12;1H") && !frames[1].contains("\x1b[13;1H"), "{:?}", frames[1]);
        assert!(frames[2].contains("row 01") && !frames[2].contains("row 00"), "{:?}", frames[2]);
    }

    #[test]
    fn a_new_height_alone_shows_more_rows_without_rendering_again() {
        let (written, widths) = paged(&TEXT_ONLY, FIRST_SIZE, vec![resize(40, 20), press('q')]);
        let frames = frames(&written);

        assert_eq!(widths, [40]);
        assert!(frames[1].contains("row 18") && frames[1].contains("\x1b[20;1H"), "{:?}", frames[1]);
    }

    #[test]
    fn keys_after_a_late_reply_opened_are_dropped_until_a_pause_gives_them_back() {
        let reply = || key(KeyCode::Char(']'), KeyModifiers::ALT);
        let escape = key(KeyCode::Esc, KeyModifiers::NONE);

        let (dropped, _) = paged(&TEXT_ONLY, FIRST_SIZE, vec![reply(), press('j'), escape]);
        let (kept, _) = paged(&TEXT_ONLY, FIRST_SIZE, vec![reply(), Scripted::Pause, press('j'), press('q')]);

        assert_eq!(frames(&dropped).len(), 1);
        assert_eq!(frames(&kept).len(), 2);
        assert!(frames(&kept)[1].contains("row 01") && !frames(&kept)[1].contains("row 00"));
    }

    #[test]
    fn a_new_width_stores_its_pictures_before_the_frame_that_shows_them() {
        let graphics = Graphics { protocol: Protocol::Kitty, cell: CellSize { width_px: 10, height_px: 20 } };
        let kitty = Capabilities { graphics: Some(graphics), ..TEXT_ONLY };

        let (written, widths) = paged(&kitty, FIRST_SIZE, vec![resize(60, 10), press('q')]);
        let stored: Vec<usize> = written.match_indices("a=t,").map(|(index, _)| index).collect();
        let drawn: Vec<usize> = written.match_indices(BEGIN_SYNCHRONIZED).map(|(index, _)| index).collect();

        assert_eq!(widths, [40, 60]);
        assert_eq!((stored.len(), drawn.len()), (2, 2));
        assert!(stored[0] < drawn[0] && drawn[0] < stored[1] && stored[1] < drawn[1], "{stored:?} {drawn:?}");
    }

    #[test]
    fn a_resize_before_the_events_were_watched_shows_in_the_first_frame() {
        let (written, widths) = paged(&TEXT_ONLY, (60, 20), vec![press('q')]);

        assert_eq!(widths, [40, 60]);
        assert_eq!(frames(&written).len(), 1);
        assert!(written.contains("\x1b[20;1H"), "{written:?}");
    }
}
