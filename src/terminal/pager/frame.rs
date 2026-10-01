use std::ops::RangeInclusive;

use super::page::{Page, Placed, Row};
use super::picture;
use super::search::{self, Match};
use super::state::Pager;
use super::status;
use crate::document::Line;
use crate::terminal::ansi::Half;
use crate::terminal::kitty::Placement;
use crate::terminal::sixel::Image;
use crate::terminal::{Capabilities, Protocol, ansi, kitty};
use crate::theme::Palette;

/// "mrk" in ASCII, so the pager's image ids stay clear of the small ids other programs pick.
const FIRST_IMAGE_ID: u32 = 0x6d72_6b00;
const LAST_IMAGE_ID: u32 = FIRST_IMAGE_ID + 0xffff;
pub const IMAGE_IDS: RangeInclusive<u32> = FIRST_IMAGE_ID..=LAST_IMAGE_ID;
/// Freed one by one as well, for terminals that ignore the range delete.
const FREED_ONE_BY_ONE: usize = 64;

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
    /// The image id of each picture of the page.
    KittyThroughTmux(Vec<u32>),
    /// Each picture of the page, `None` when its PNG cannot be read.
    Sixel(Vec<Option<Image>>),
}

impl Pictures {
    pub fn new(page: &Page, protocol: Option<Protocol>) -> Self {
        let pngs = page.pictures.iter().map(|placed| placed.picture.png.as_slice());
        match protocol {
            None => Self::Off,
            Some(Protocol::Kitty) => Self::Kitty,
            Some(Protocol::KittyThroughTmux) => Self::KittyThroughTmux(pngs.map(kitty::placeholder_id).collect()),
            Some(Protocol::Sixel) => Self::Sixel(pngs.map(Image::from_png).collect()),
        }
    }

    /// What to send once per render, before any frame: the kitty pictures, after freeing the last render's; through
    /// tmux their ids follow their PNG, so sending one again replaces it.
    pub fn store(&self, page: &Page) -> String {
        match self {
            Self::Kitty => forget_pictures() + &store_pictures(page),
            Self::KittyThroughTmux(ids) => {
                page.pictures.iter().zip(ids).map(|(placed, &id)| kitty::store_through_tmux(&placed.picture, id)).collect()
            }
            Self::Off | Self::Sixel(_) => String::new(),
        }
    }

    /// The placeholder cells of row `row` of the picture at `index`, through tmux; nothing otherwise.
    fn cells(&self, index: usize, placed: &Placed, row: usize) -> String {
        let Self::KittyThroughTmux(ids) = self else { return String::new() };
        ids.get(index).and_then(|&id| kitty::placeholder_row(id, row, placed.picture.cols)).unwrap_or_default()
    }
}

fn image_id(index: usize) -> u32 {
    FIRST_IMAGE_ID + (index as u32 & 0xffff)
}

fn go_to(row: usize, column: usize) -> String {
    format!("\x1b[{};{}H", row + 1, column + 1)
}

fn row_matches(matches: &[Match], row: usize) -> &[Match] {
    let start = matches.partition_point(|each| each.row < row);
    let end = matches.partition_point(|each| each.row <= row);
    &matches[start..end]
}

fn drawn_line(line: &Line, look: &Look) -> String {
    ansi::line(line, look.capabilities, look.margin).trim_end_matches('\n').to_owned()
}

fn drawn_half(line: &Line, half: Half, look: &Look) -> String {
    ansi::half(line, half, look.capabilities, look.margin).trim_end_matches('\n').to_owned()
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
        Some(Row::DoubleHeight(line, half)) => drawn_half(line, *half, look),
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
    format!("{}{}", row_start(pager.height, look), ansi::line(&bar, look.capabilities, 0).trim_end_matches('\n'))
}

fn hidden_pictures(page: &Page) -> String {
    (0..page.pictures.len()).map(|index| kitty::hide(image_id(index))).collect()
}

/// Each picture on screen: its index, the cursor move to its first visible cell, and what of it shows.
fn on_screen<'a>(pager: &'a Pager, look: &'a Look) -> impl Iterator<Item = (usize, String, Placement)> + 'a {
    pager.page.pictures.iter().enumerate().filter_map(|(index, placed)| {
        let (screen_row, placement) = picture::placement(placed, image_id(index), pager.top, pager.height)?;
        Some((index, go_to(screen_row, look.margin + placed.picture.indent.width()), placement))
    })
}

fn placed_pictures(pager: &Pager, look: &Look) -> String {
    on_screen(pager, look).map(|(_, at, placement)| at + &kitty::place(&placement)).collect()
}

fn sixel_pictures(pager: &Pager, look: &Look, images: &[Option<Image>]) -> String {
    let drawn = |(index, at, placement): (usize, String, Placement)| {
        let image = images.get(index)?.as_ref()?;
        Some(at + &image.encode(placement.crop.y..placement.crop.y + placement.crop.height))
    };
    on_screen(pager, look).filter_map(drawn).collect()
}

/// One whole screen, drawn at once: last frame's kitty pictures taken off, every text row and the status bar written
/// over, which also erases last frame's Sixel pictures, then the pictures on screen drawn again, clipped to the rows
/// that show.
pub fn frame(pager: &Pager, look: &Look, pictures: &Pictures) -> String {
    let (hidden, drawn) = match pictures {
        Pictures::Off | Pictures::KittyThroughTmux(_) => (String::new(), String::new()),
        Pictures::Kitty => (hidden_pictures(&pager.page), placed_pictures(pager, look)),
        Pictures::Sixel(images) => (String::new(), sixel_pictures(pager, look, images)),
    };
    [BEGIN_SYNCHRONIZED, &hidden, &text_rows(pager, look, pictures), &status_row(pager, look), &drawn, END_SYNCHRONIZED].concat()
}

/// Sends every picture of the page to the terminal once, so frames only place them.
pub fn store_pictures(page: &Page) -> String {
    page.pictures.iter().enumerate().map(|(index, placed)| kitty::store(&placed.picture.png, image_id(index))).collect()
}

/// Frees every picture the pager may have sent.
pub fn forget_pictures() -> String {
    let one_by_one: String = (0..FREED_ONE_BY_ONE).map(|index| kitty::free(image_id(index))).collect();
    kitty::forget(IMAGE_IDS) + &one_by_one
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Block, Document, Picture, Span, Style};
    use crate::terminal::ColorDepth;
    use crate::terminal::pager::keys::Action;
    use crate::terminal::pager::page::flatten;
    use crate::theme::MRK_DARK;

    const CAPABILITIES: Capabilities = Capabilities {
        color: ColorDepth::TrueColor,
        hyperlinks: true,
        graphics: None,
        double_height: false,
        background: None,
        columns: 40,
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

    fn drawn(pager: &Pager, protocol: Option<Protocol>) -> String {
        drawn_on(pager, protocol, &CAPABILITIES)
    }

    fn drawn_on(pager: &Pager, protocol: Option<Protocol>, capabilities: &Capabilities) -> String {
        let look = Look { name: "a.md", palette: &MRK_DARK.palette, capabilities, margin: 2, columns: 40 };
        frame(pager, &look, &Pictures::new(&pager.page, protocol))
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
        assert!(pictures.starts_with("0;1;0q\"1;1;200;36#"), "{pictures:?}");
        assert!(!frame.contains("\x1b_G"), "{frame:?}");
    }

    #[test]
    fn through_tmux_the_visible_picture_rows_are_placeholder_cells_numbered_from_the_hidden_ones() {
        let pager = indented_sample(text("│ "));
        let pictures = Pictures::new(&pager.page, Some(Protocol::KittyThroughTmux));
        let Pictures::KittyThroughTmux(ids) = &pictures else { panic!("{pictures:?}") };

        let frame = drawn(&pager, Some(Protocol::KittyThroughTmux));

        assert!(frame.contains(&kitty::placeholder_row(ids[0], 1, 20).unwrap_or_default()), "{frame:?}");
        assert!(frame.contains(&kitty::placeholder_row(ids[0], 2, 20).unwrap_or_default()), "{frame:?}");
        assert!(!frame.contains(&kitty::placeholder_row(ids[0], 0, 20).unwrap_or_default()), "{frame:?}");
        assert!(!frame.contains("\x1b_G") && !frame.contains("\x1bP"), "{frame:?}");
        assert_eq!(pictures.store(&pager.page).matches("\x1bPtmux;").count(), 1);
    }

    #[test]
    fn an_unreadable_sixel_picture_is_left_out() {
        let page =
            flatten(Document { blocks: vec![Block::Picture(Picture { png: vec![1, 2, 3], ..sample().page.pictures[0].picture.clone() })] });

        assert!(matches!(Pictures::new(&page, Some(Protocol::Sixel)), Pictures::Sixel(images) if images.iter().all(Option::is_none)));
        assert!(!drawn(&Pager::new(page, 3), Some(Protocol::Sixel)).contains("\x1bP"));
    }

    #[test]
    fn a_hostile_query_is_drawn_sanitized() {
        let typed =
            [Action::StartSearch, Action::Type('\x1b'), Action::Type(']'), Action::Type('\x07')].iter().fold(sample(), Pager::apply);

        let frame = drawn(&typed, None);

        assert!(!frame.contains('\x07') && !frame.contains("\x1b]"), "{frame:?}");
        assert!(frame.contains("m]\x1b["), "{frame:?}");
    }

    #[test]
    fn stored_pictures_are_sent_once_each_under_their_id() {
        let stored = store_pictures(&sample().page);

        assert_eq!(stored.matches("a=t,").count(), 1);
        assert!(stored.contains(&format!("i={FIRST_IMAGE_ID},")));
    }
}
