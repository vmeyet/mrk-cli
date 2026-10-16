use std::ops::Range;

use super::page::{Page, Placed, Row};
use super::picture;
use super::search::{self, Match};
use super::state::Pager;
use super::status;
use crate::document::Line;
use crate::terminal::kitty::Placement;
use crate::terminal::sixel::Image;
use crate::terminal::{Capabilities, ColorDepth, Protocol, ansi, kitty, tmux};
use crate::theme::Palette;

const BEGIN_SYNCHRONIZED: &str = "\x1b[?2026h";
const END_SYNCHRONIZED: &str = "\x1b[?2026l";
const CLEAR_LINE: &str = "\x1b[2K";

/// How the pager looks, besides what it shows.
#[derive(Clone, Copy, Debug)]
pub struct Look<'a> {
    pub name: &'a str,
    pub palette: &'a Palette,
    pub capabilities: &'a Capabilities,
    pub margin: usize,
    pub columns: usize,
}

/// The page's pictures as the terminal draws them: kitty keeps them once sent and only places them, through tmux
/// they show where their placeholder cells are written, Sixel has no ids, so each frame sends again what shows.
#[derive(Clone, Debug)]
pub enum Pictures {
    Off,
    Kitty,
    /// Under the ids of the pager of process `pid`, which placeholder cells carry in `color`.
    KittyThroughTmux {
        pid: u32,
        color: ColorDepth,
    },
    /// Each picture of the page, `None` when its PNG cannot be read.
    Sixel(Vec<Option<SixelPicture>>),
}

/// A picture reduced to Sixel colours, and the rows of it the last frame sent, kept so a frame where it did not move
/// sends the same bytes without encoding them again.
#[derive(Clone, Debug)]
pub struct SixelPicture {
    image: Image,
    sent: Option<(Range<u32>, String)>,
}

impl SixelPicture {
    fn with_sent(self, rows: Option<Range<u32>>) -> Self {
        let Some(rows) = rows else { return self };
        if self.sent.as_ref().is_some_and(|(sent_rows, _)| *sent_rows == rows) {
            return self;
        }
        let sixel = self.image.encode(rows.clone());
        Self { sent: Some((rows, sixel)), ..self }
    }
}

impl Pictures {
    /// The page's pictures for this terminal, in the pager of process `pid`.
    pub fn new(page: &Page, capabilities: &Capabilities, pid: u32) -> Self {
        let sixel = |placed: &Placed| Some(SixelPicture { image: Image::from_png(&placed.picture.png)?, sent: None });
        match capabilities.graphics.map(|graphics| graphics.protocol) {
            None => Self::Off,
            Some(Protocol::Kitty) => Self::Kitty,
            Some(Protocol::KittyThroughTmux) => Self::KittyThroughTmux { pid, color: capabilities.color },
            Some(Protocol::Sixel) => Self::Sixel(page.pictures.iter().map(sixel).collect()),
        }
    }

    /// What to send once per render, before any frame: the kitty pictures, under the pager's ids.
    pub fn store(&self, page: &Page) -> String {
        let with_ids = self.with_ids(page);
        match self {
            Self::Kitty => with_ids.map(|(id, placed)| kitty::store(&placed.picture.png, id)).collect(),
            Self::KittyThroughTmux { .. } => with_ids.map(|(id, placed)| kitty::store_through_tmux(&placed.picture, id)).collect(),
            Self::Off | Self::Sixel(_) => String::new(),
        }
    }

    /// What frees exactly the pictures `store` sent for `page`, through tmux passed on to the terminal outside; never a
    /// range, which could free another program's pictures.
    pub fn free(&self, page: &Page) -> String {
        let ids = self.with_ids(page).map(|(id, _)| id);
        match self {
            Self::Kitty => ids.map(kitty::free).collect(),
            Self::KittyThroughTmux { .. } => ids.map(|id| tmux::passthrough(&kitty::free(id))).collect(),
            Self::Off | Self::Sixel(_) => String::new(),
        }
    }

    /// The Sixel pictures with the rows this screen shows of each encoded, the last frame's kept where it did not move.
    pub fn encoded(self, pager: &Pager) -> Self {
        let Self::Sixel(pictures) = self else { return self };
        let shown = |placed: &Placed| picture::placement(placed, 0, pager.top, pager.height).map(|(_, at)| at.crop.pixel_rows());
        Self::Sixel(
            pictures.into_iter().zip(&pager.page.pictures).map(|(each, placed)| each.map(|each| each.with_sent(shown(placed)))).collect(),
        )
    }

    /// The placeholder cells of row `row` of the picture at `index`, through tmux; nothing otherwise.
    fn cells(&self, index: usize, placed: &Placed, row: usize) -> String {
        if !matches!(self, Self::KittyThroughTmux { .. }) {
            return String::new();
        }
        self.id(index).and_then(|id| kitty::placeholder_row(id, row, placed.picture.cols)).unwrap_or_default()
    }

    /// The kitty id of the picture at `index`. Kitty keeps ids per window, so outside tmux they only stay clear of the
    /// small ids other programs pick; through tmux they follow the process (`kitty::pager_id`).
    fn id(&self, index: usize) -> Option<u32> {
        match *self {
            Self::Kitty => FIRST_IMAGE_ID.checked_add(u32::try_from(index).ok()?),
            Self::KittyThroughTmux { pid, color } => kitty::pager_id(pid, index, color),
            Self::Off | Self::Sixel(_) => None,
        }
    }

    /// The page's pictures that have an id, with it.
    fn with_ids<'a>(&'a self, page: &'a Page) -> impl Iterator<Item = (u32, &'a Placed)> + 'a {
        page.pictures.iter().enumerate().filter_map(|(index, placed)| Some((self.id(index)?, placed)))
    }
}

/// "mrk" in ASCII.
const FIRST_IMAGE_ID: u32 = 0x6d72_6b00;

fn go_to(row: usize, column: usize) -> String {
    format!("\x1b[{};{}H", row + 1, column + 1)
}

fn row_matches(matches: &[Match], row: usize) -> &[Match] {
    let start = matches.partition_point(|each| each.row < row);
    let end = matches.partition_point(|each| each.row <= row);
    &matches[start..end]
}

fn drawn_line(line: &Line, look: &Look) -> String {
    ansi::line(line, look.capabilities, look.margin)
}

fn picture_row(pager: &Pager, picture: usize, row: usize, look: &Look, pictures: &Pictures) -> String {
    let Some(placed) = pager.page.pictures.get(picture) else { return String::new() };
    let concealed = if row == placed.row { ansi::concealed(&placed.picture) } else { String::new() };
    drawn_line(&placed.picture.indent, look) + &concealed + &pictures.cells(picture, placed, row - placed.row)
}

fn row_text(pager: &Pager, index: usize, look: &Look, pictures: &Pictures) -> String {
    let matches = row_matches(&pager.search.matches, index);
    match pager.page.rows.get(index) {
        Some(Row::Line(line)) if matches.is_empty() => drawn_line(line, look),
        Some(Row::Line(line)) => drawn_line(&search::highlight(line, matches, pager.search.current_match(), look.palette), look),
        Some(Row::Picture(picture)) => picture_row(pager, *picture, index, look, pictures),
        Some(Row::DoubleHeight(line, half)) => ansi::half(line, *half, look.capabilities, look.margin),
        None => String::new(),
    }
}

/// The cursor at the start of the row, which a double-height line drawn there last frame is taken off.
fn row_start(row: usize, look: &Look) -> String {
    let single_width = if look.capabilities.double_height { ansi::SINGLE_WIDTH } else { "" };
    format!("{}{single_width}", go_to(row, 0))
}

fn text_rows(pager: &Pager, look: &Look, pictures: &Pictures) -> String {
    (0..pager.height)
        .map(|screen_row| format!("{}{CLEAR_LINE}{}", row_start(screen_row, look), row_text(pager, pager.top + screen_row, look, pictures)))
        .collect()
}

fn status_row(pager: &Pager, look: &Look) -> String {
    let bar = status::bar(pager, look.name, look.palette, look.columns);
    format!("{}{}", row_start(pager.height, look), ansi::line(&bar, look.capabilities, 0))
}

fn hidden_pictures(page: &Page, pictures: &Pictures) -> String {
    pictures.with_ids(page).map(|(id, _)| kitty::hide(id)).collect()
}

/// Each picture on screen: its index, the cursor move to its first visible cell, and what of it shows, under no id
/// yet, since only kitty's have one.
fn on_screen<'a>(pager: &'a Pager, look: &'a Look) -> impl Iterator<Item = (usize, String, Placement)> + 'a {
    pager.page.pictures.iter().enumerate().filter_map(|(index, placed)| {
        let (screen_row, placement) = picture::placement(placed, 0, pager.top, pager.height)?;
        Some((index, go_to(screen_row, look.margin + placed.picture.indent.width()), placement))
    })
}

fn placed_pictures(pager: &Pager, look: &Look, pictures: &Pictures) -> String {
    let placed =
        |(index, at, placement): (usize, String, Placement)| Some(at + &kitty::place(&Placement { id: pictures.id(index)?, ..placement }));
    on_screen(pager, look).filter_map(placed).collect()
}

fn sixel_pictures(pager: &Pager, look: &Look, pictures: &[Option<SixelPicture>]) -> String {
    let drawn = |(index, at, placement): (usize, String, Placement)| {
        let (rows, sixel) = pictures.get(index)?.as_ref()?.sent.as_ref()?;
        (*rows == placement.crop.pixel_rows()).then(|| at + sixel)
    };
    on_screen(pager, look).filter_map(drawn).collect()
}

/// One whole screen, drawn at once, with the Sixel pictures `Pictures::encoded` for it: last frame's kitty pictures
/// taken off, every text row and the status bar written over, which also erases last frame's Sixel pictures, then the
/// pictures on screen drawn again, clipped to the rows that show.
pub fn frame(pager: &Pager, look: &Look, pictures: &Pictures) -> String {
    let (hidden, drawn) = match pictures {
        Pictures::Off | Pictures::KittyThroughTmux { .. } => (String::new(), String::new()),
        Pictures::Kitty => (hidden_pictures(&pager.page, pictures), placed_pictures(pager, look, pictures)),
        Pictures::Sixel(each) => (String::new(), sixel_pictures(pager, look, each)),
    };
    [BEGIN_SYNCHRONIZED, &hidden, &text_rows(pager, look, pictures), &status_row(pager, look), &drawn, END_SYNCHRONIZED].concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::CellSize;
    use crate::document::{Block, Document, Picture, Span, Style};
    use crate::terminal::Graphics;
    use crate::terminal::pager::keys::Action;
    use crate::terminal::pager::page::flatten;
    use crate::theme::MRK_DARK;

    const PID: u32 = 70_807;
    const ID: u32 = 0x0081_1497;

    const CAPABILITIES: Capabilities = Capabilities {
        color: ColorDepth::TrueColor,
        hyperlinks: true,
        graphics: None,
        double_height: false,
        background: None,
        columns: 40,
        rows: 10,
        is_terminal: true,
    };

    fn png() -> Vec<u8> {
        crate::terminal::sixel::test_png(200, 60, &vec![[255, 0, 0, 255]; 200 * 60])
    }

    fn text(value: &str) -> Line {
        Line::new(vec![Span::new(value, Style::fg(MRK_DARK.palette.text))])
    }

    fn sample() -> Pager {
        indented_sample(Line::blank())
    }

    fn indented_sample(indent: Line) -> Pager {
        let picture = Picture { png: png(), cols: 20, rows: 3, alt: "flow".to_owned(), indent, concealed_text: None };
        let document = Document {
            blocks: vec![Block::Lines(vec![text("Alpha"), text("beta")]), Block::Picture(picture), Block::Lines(vec![text("gamma")])],
        };
        Pager { top: 3, ..Pager::new(flatten(document), 3) }
    }

    fn readable(frame: &str) -> String {
        frame.replace('\x1b', "␛").replace("␛[", "\n␛[")
    }

    fn pictures_on(page: &Page, protocol: Option<Protocol>, capabilities: &Capabilities) -> Pictures {
        let graphics = protocol.map(|protocol| Graphics { protocol, cell: CellSize { width_px: 10, height_px: 20 } });
        Pictures::new(page, &Capabilities { graphics, ..*capabilities }, PID)
    }

    fn pictures(page: &Page, protocol: Option<Protocol>) -> Pictures {
        pictures_on(page, protocol, &CAPABILITIES)
    }

    fn drawn(pager: &Pager, protocol: Option<Protocol>) -> String {
        drawn_on(pager, protocol, &CAPABILITIES)
    }

    fn look(capabilities: &Capabilities) -> Look<'_> {
        Look { name: "a.md", palette: &MRK_DARK.palette, capabilities, margin: 2, columns: 40 }
    }

    fn drawn_on(pager: &Pager, protocol: Option<Protocol>, capabilities: &Capabilities) -> String {
        frame(pager, &look(capabilities), &pictures_on(&pager.page, protocol, capabilities).encoded(pager))
    }

    #[test]
    fn a_title_picture_is_placed_under_its_concealed_text() {
        let title = Picture { concealed_text: Some("Title".to_owned()), ..sample().page.pictures[0].picture.clone() };
        let frame = drawn(&Pager::new(flatten(Document { blocks: vec![Block::Picture(title)] }), 3), Some(Protocol::Kitty));

        assert!(frame.contains("\x1b[8mTitle\x1b[28m"), "{frame:?}");
        assert!(frame.contains(",z=-1,"), "{frame:?}");
    }

    #[test]
    fn double_height_rows_are_drawn_as_halves_and_every_other_row_set_back_to_single_width() {
        let document = Document { blocks: vec![Block::DoubleHeight(vec![text("Big")]), Block::Lines(vec![text("small")])] };
        let pager = Pager::new(flatten(document), 3);
        let capabilities = Capabilities { double_height: true, ..CAPABILITIES };

        let frame = drawn_on(&pager, None, &capabilities);

        assert!(frame.contains("\x1b[1;1H\x1b#5\x1b[2K\x1b#3 \x1b["), "{frame:?}");
        assert!(frame.contains("\x1b[2;1H\x1b#5\x1b[2K\x1b#4 \x1b["), "{frame:?}");
        assert!(frame.contains("\x1b[3;1H\x1b#5\x1b[2K  \x1b["), "{frame:?}");
        assert!(!drawn(&pager, None).contains("\x1b#5"));
    }

    #[test]
    fn a_frame_with_a_picture_clipped_at_the_top() {
        insta::assert_snapshot!(readable(&drawn(&sample(), Some(Protocol::Kitty))));
    }

    #[test]
    fn a_picture_is_placed_after_its_indent_drawn_on_each_visible_row() {
        let frame = drawn(&indented_sample(text("│ ")), Some(Protocol::Kitty));

        assert_eq!(frame.matches("│ ").count(), 2, "{frame:?}");
        assert!(frame.contains("\x1b[1;5H\x1b_Ga=p,"), "{frame:?}");
    }

    #[test]
    fn without_graphics_no_picture_command_is_sent() {
        let frame = drawn(&sample(), None);

        assert!(!frame.contains("\x1b_G") && !frame.contains("\x1bP"), "{frame:?}");
    }

    #[test]
    fn a_sixel_frame_sends_the_visible_band_again_after_the_rows_that_erase_it() {
        let frame = drawn(&indented_sample(text("│ ")), Some(Protocol::Sixel));
        let (rows, pictures) = frame.split_once("\x1bP").unwrap_or_default();

        assert!(rows.contains("\x1b[2K") && rows.ends_with("\x1b[1;5H"), "{rows:?}");
        assert!(pictures.starts_with("0;1;0q\"1;1;200;40#"), "{pictures:?}");
        assert!(!frame.contains("\x1b_G"), "{frame:?}");
    }

    #[test]
    fn through_tmux_the_visible_picture_rows_are_placeholder_cells_numbered_from_the_hidden_ones() {
        let pager = indented_sample(text("│ "));
        let pictures = pictures(&pager.page, Some(Protocol::KittyThroughTmux));

        let frame = drawn(&pager, Some(Protocol::KittyThroughTmux));

        assert!(frame.contains(&kitty::placeholder_row(ID, 1, 20).unwrap_or_default()), "{frame:?}");
        assert!(frame.contains(&kitty::placeholder_row(ID, 2, 20).unwrap_or_default()), "{frame:?}");
        assert!(!frame.contains(&kitty::placeholder_row(ID, 0, 20).unwrap_or_default()), "{frame:?}");
        assert!(!frame.contains("\x1b_G") && !frame.contains("\x1bP"), "{frame:?}");
        assert_eq!(pictures.store(&pager.page).matches(&format!("\x1bPtmux;\x1b\x1b_Ga=T,U=1,f=100,i={ID},")).count(), 1);
    }

    #[test]
    fn exactly_the_pictures_sent_are_freed_and_through_tmux_passed_on() {
        let page = sample().page;
        let kitty = pictures(&page, Some(Protocol::Kitty)).free(&page);
        let through_tmux = pictures(&page, Some(Protocol::KittyThroughTmux)).free(&page);

        assert_eq!(kitty, format!("\x1b_Ga=d,d=I,i={FIRST_IMAGE_ID},q=2\x1b\\"));
        assert_eq!(through_tmux, format!("\x1bPtmux;\x1b\x1b_Ga=d,d=I,i={ID},q=2\x1b\x1b\\\x1b\\"));
    }

    #[test]
    fn through_tmux_nothing_is_sent_past_the_256th_picture_but_kitty_takes_them_all() {
        let picture = sample().page.pictures[0].picture.clone();
        let page = flatten(Document { blocks: vec![Block::Picture(picture); 257] });

        let through_tmux = pictures(&page, Some(Protocol::KittyThroughTmux)).store(&page);
        let kitty = pictures(&page, Some(Protocol::Kitty)).store(&page);

        assert_eq!(through_tmux.matches("a=T,U=1,").count(), 256);
        assert_eq!(kitty.matches("a=t,").count(), 257);
        assert!(kitty.contains(&format!("i={},", FIRST_IMAGE_ID + 256)));
    }

    #[test]
    fn without_true_colour_the_placeholder_cells_carry_a_256_colour_id() {
        let pager = indented_sample(text("│ "));
        let capabilities = Capabilities { color: ColorDepth::Ansi256, ..CAPABILITIES };

        let frame = drawn_on(&pager, Some(Protocol::KittyThroughTmux), &capabilities);

        assert!(frame.contains("\x1b[38;5;173m\u{10eeee}"), "{frame:?}");
    }

    #[test]
    fn nothing_is_freed_where_no_kitty_picture_was_sent() {
        let page = sample().page;

        for protocol in [None, Some(Protocol::Sixel)] {
            assert_eq!(pictures(&page, protocol).free(&page), "", "{protocol:?}");
        }
        assert_eq!(pictures(&Page::default(), Some(Protocol::Kitty)).free(&Page::default()), "");
    }

    #[test]
    fn a_sixel_picture_that_did_not_move_is_sent_as_last_frame_encoded_it() {
        let pager = indented_sample(text("│ "));
        let pictures = pictures(&pager.page, Some(Protocol::Sixel)).encoded(&pager);
        let Pictures::Sixel(each) = &pictures else { panic!("{pictures:?}") };
        let Some(SixelPicture { sent: Some((rows, sixel)), .. }) = &each[0] else { panic!("{each:?}") };

        assert_eq!(*rows, 20..60);
        assert!(frame(&pager, &look(&CAPABILITIES), &pictures).contains(sixel.as_str()));
        let scrolled = Pager { top: 2, ..pager };
        let Pictures::Sixel(moved) = pictures.encoded(&scrolled) else { unreachable!() };
        assert!(matches!(&moved[0], Some(SixelPicture { sent: Some((rows, _)), .. }) if *rows == (0..60)));
    }

    #[test]
    fn an_unreadable_sixel_picture_is_left_out() {
        let page =
            flatten(Document { blocks: vec![Block::Picture(Picture { png: vec![1, 2, 3], ..sample().page.pictures[0].picture.clone() })] });

        assert!(matches!(pictures(&page, Some(Protocol::Sixel)), Pictures::Sixel(images) if images.iter().all(Option::is_none)));
        assert!(!drawn(&Pager::new(page, 3), Some(Protocol::Sixel)).contains("\x1bP"));
    }

    #[test]
    fn a_hostile_query_is_drawn_sanitized() {
        let typed =
            [Action::StartSearch, Action::Type('\x1b'), Action::Type(']'), Action::Type('\x07')].into_iter().fold(sample(), Pager::apply);

        let frame = drawn(&typed, None);

        assert!(!frame.contains('\x07') && !frame.contains("\x1b]"), "{frame:?}");
        assert!(frame.contains("m]\x1b["), "{frame:?}");
    }

    #[test]
    fn stored_pictures_are_sent_once_each_under_their_id() {
        let page = sample().page;
        let stored = pictures(&page, Some(Protocol::Kitty)).store(&page);

        assert_eq!(stored.matches("a=t,").count(), 1);
        assert!(stored.contains(&format!("i={FIRST_IMAGE_ID},")));
    }
}
