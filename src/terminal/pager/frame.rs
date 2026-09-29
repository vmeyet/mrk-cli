use std::ops::RangeInclusive;

use super::page::{Page, Placed, Row};
use super::picture;
use super::search::{self, Match};
use super::state::Pager;
use super::status;
use crate::document::Line;
use crate::terminal::{Capabilities, ansi, kitty};
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

fn row_text(pager: &Pager, index: usize, look: &Look) -> String {
    let matches = row_matches(&pager.search.matches, index);
    match pager.page.rows.get(index) {
        Some(Row::Line(line)) if matches.is_empty() => drawn_line(line, look),
        Some(Row::Line(line)) => drawn_line(&search::highlight(line, matches, pager.search.current_match(), look.palette), look),
        Some(Row::Picture(index)) => {
            pager.page.pictures.get(*index).map_or_else(String::new, |placed| drawn_line(&placed.picture.indent, look))
        }
        None => String::new(),
    }
}

fn text_rows(pager: &Pager, look: &Look) -> String {
    (0..pager.height)
        .map(|screen_row| format!("{}{CLEAR_LINE}{}", go_to(screen_row, 0), row_text(pager, pager.top + screen_row, look)))
        .collect()
}

fn status_row(pager: &Pager, look: &Look) -> String {
    let bar = status::bar(pager, look.name, look.palette, look.columns);
    format!("{}{}", go_to(pager.height, 0), ansi::line(&bar, look.capabilities, 0).trim_end_matches('\n'))
}

fn hidden_pictures(page: &Page) -> String {
    (0..page.pictures.len()).map(|index| kitty::hide(image_id(index))).collect()
}

fn placed_pictures(pager: &Pager, look: &Look) -> String {
    let placed = |(index, placed): (usize, &Placed)| {
        let (screen_row, placement) = picture::placement(placed, image_id(index), pager.top, pager.height)?;
        Some(format!("{}{}", go_to(screen_row, look.margin + placed.picture.indent.width()), kitty::place(&placement)))
    };
    pager.page.pictures.iter().enumerate().filter_map(placed).collect()
}

/// One whole screen, drawn at once: last frame's pictures taken off, every text row and the status bar written over,
/// then the pictures on screen placed again, clipped to the rows that show.
pub fn frame(pager: &Pager, look: &Look) -> String {
    let has_pictures = look.capabilities.cell.is_some();
    let hidden = if has_pictures { hidden_pictures(&pager.page) } else { String::new() };
    let placed = if has_pictures { placed_pictures(pager, look) } else { String::new() };
    [BEGIN_SYNCHRONIZED, &hidden, &text_rows(pager, look), &status_row(pager, look), &placed, END_SYNCHRONIZED].concat()
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
    use crate::document::{Block, CellSize, Document, Picture, Span, Style};
    use crate::terminal::ColorDepth;
    use crate::terminal::pager::keys::Action;
    use crate::terminal::pager::page::flatten;
    use crate::theme::MRK_DARK;

    fn capabilities(cell: Option<CellSize>) -> Capabilities {
        Capabilities { color: ColorDepth::TrueColor, hyperlinks: true, cell, background: None, columns: 40, is_terminal: true }
    }

    fn png() -> Vec<u8> {
        [b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".as_slice(), &200_u32.to_be_bytes(), &60_u32.to_be_bytes()].concat()
    }

    fn text(value: &str) -> Line {
        Line::new(vec![Span::new(value, Style::fg(MRK_DARK.palette.text))])
    }

    fn sample() -> Pager {
        indented_sample(Line::blank())
    }

    fn indented_sample(indent: Line) -> Pager {
        let picture = Picture { png: png(), cols: 20, rows: 3, alt: "flow".to_owned(), indent };
        let document = Document {
            blocks: vec![Block::Lines(vec![text("Alpha"), text("beta")]), Block::Picture(picture), Block::Lines(vec![text("gamma")])],
        };
        Pager { top: 3, ..Pager::new(flatten(document), 3) }
    }

    fn readable(frame: &str) -> String {
        frame.replace('\x1b', "␛").replace("␛[", "\n␛[")
    }

    fn drawn(pager: &Pager, cell: Option<CellSize>) -> String {
        let look = Look { name: "a.md", palette: &MRK_DARK.palette, capabilities: &capabilities(cell), margin: 2, columns: 40 };
        frame(pager, &look)
    }

    #[test]
    fn a_frame_with_a_picture_clipped_at_the_top() {
        let cell = Some(CellSize { width_px: 10, height_px: 20 });

        insta::assert_snapshot!(readable(&drawn(&sample(), cell)));
    }

    #[test]
    fn a_picture_is_placed_after_its_indent_drawn_on_each_visible_row() {
        let cell = Some(CellSize { width_px: 10, height_px: 20 });

        let frame = drawn(&indented_sample(text("│ ")), cell);

        assert_eq!(frame.matches("│ ").count(), 2, "{frame:?}");
        assert!(frame.contains("\x1b[1;5H\x1b_Ga=p,"), "{frame:?}");
    }

    #[test]
    fn without_graphics_no_picture_command_is_sent() {
        assert!(!drawn(&sample(), None).contains("\x1b_G"));
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
