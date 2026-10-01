use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::tmux;
use crate::document::Picture;

const CHUNK_BYTES: usize = 4096;
const PLACEMENT_ID: u32 = 1;
/// The cell kitty draws a piece of a virtual placement in; its diacritics say which row, and its colour which image.
const PLACEHOLDER: char = '\u{10eeee}';
/// The combining marks that number placeholder rows and columns, in kitty's order (`rowcolumn-diacritics.txt`).
#[rustfmt::skip]
const DIACRITICS: [char; 297] = [
    '\u{0305}', '\u{030d}', '\u{030e}', '\u{0310}', '\u{0312}', '\u{033d}', '\u{033e}', '\u{033f}', '\u{0346}', '\u{034a}', '\u{034b}',
    '\u{034c}', '\u{0350}', '\u{0351}', '\u{0352}', '\u{0357}', '\u{035b}', '\u{0363}', '\u{0364}', '\u{0365}', '\u{0366}', '\u{0367}',
    '\u{0368}', '\u{0369}', '\u{036a}', '\u{036b}', '\u{036c}', '\u{036d}', '\u{036e}', '\u{036f}', '\u{0483}', '\u{0484}', '\u{0485}',
    '\u{0486}', '\u{0487}', '\u{0592}', '\u{0593}', '\u{0594}', '\u{0595}', '\u{0597}', '\u{0598}', '\u{0599}', '\u{059c}', '\u{059d}',
    '\u{059e}', '\u{059f}', '\u{05a0}', '\u{05a1}', '\u{05a8}', '\u{05a9}', '\u{05ab}', '\u{05ac}', '\u{05af}', '\u{05c4}', '\u{0610}',
    '\u{0611}', '\u{0612}', '\u{0613}', '\u{0614}', '\u{0615}', '\u{0616}', '\u{0617}', '\u{0657}', '\u{0658}', '\u{0659}', '\u{065a}',
    '\u{065b}', '\u{065d}', '\u{065e}', '\u{06d6}', '\u{06d7}', '\u{06d8}', '\u{06d9}', '\u{06da}', '\u{06db}', '\u{06dc}', '\u{06df}',
    '\u{06e0}', '\u{06e1}', '\u{06e2}', '\u{06e4}', '\u{06e7}', '\u{06e8}', '\u{06eb}', '\u{06ec}', '\u{0730}', '\u{0732}', '\u{0733}',
    '\u{0735}', '\u{0736}', '\u{073a}', '\u{073d}', '\u{073f}', '\u{0740}', '\u{0741}', '\u{0743}', '\u{0745}', '\u{0747}', '\u{0749}',
    '\u{074a}', '\u{07eb}', '\u{07ec}', '\u{07ed}', '\u{07ee}', '\u{07ef}', '\u{07f0}', '\u{07f1}', '\u{07f3}', '\u{0816}', '\u{0817}',
    '\u{0818}', '\u{0819}', '\u{081b}', '\u{081c}', '\u{081d}', '\u{081e}', '\u{081f}', '\u{0820}', '\u{0821}', '\u{0822}', '\u{0823}',
    '\u{0825}', '\u{0826}', '\u{0827}', '\u{0829}', '\u{082a}', '\u{082b}', '\u{082c}', '\u{082d}', '\u{0951}', '\u{0953}', '\u{0954}',
    '\u{0f82}', '\u{0f83}', '\u{0f86}', '\u{0f87}', '\u{135d}', '\u{135e}', '\u{135f}', '\u{17dd}', '\u{193a}', '\u{1a17}', '\u{1a75}',
    '\u{1a76}', '\u{1a77}', '\u{1a78}', '\u{1a79}', '\u{1a7a}', '\u{1a7b}', '\u{1a7c}', '\u{1b6b}', '\u{1b6d}', '\u{1b6e}', '\u{1b6f}',
    '\u{1b70}', '\u{1b71}', '\u{1b72}', '\u{1b73}', '\u{1cd0}', '\u{1cd1}', '\u{1cd2}', '\u{1cda}', '\u{1cdb}', '\u{1ce0}', '\u{1dc0}',
    '\u{1dc1}', '\u{1dc3}', '\u{1dc4}', '\u{1dc5}', '\u{1dc6}', '\u{1dc7}', '\u{1dc8}', '\u{1dc9}', '\u{1dcb}', '\u{1dcc}', '\u{1dd1}',
    '\u{1dd2}', '\u{1dd3}', '\u{1dd4}', '\u{1dd5}', '\u{1dd6}', '\u{1dd7}', '\u{1dd8}', '\u{1dd9}', '\u{1dda}', '\u{1ddb}', '\u{1ddc}',
    '\u{1ddd}', '\u{1dde}', '\u{1ddf}', '\u{1de0}', '\u{1de1}', '\u{1de2}', '\u{1de3}', '\u{1de4}', '\u{1de5}', '\u{1de6}', '\u{1dfe}',
    '\u{20d0}', '\u{20d1}', '\u{20d4}', '\u{20d5}', '\u{20d6}', '\u{20d7}', '\u{20db}', '\u{20dc}', '\u{20e1}', '\u{20e7}', '\u{20e9}',
    '\u{20f0}', '\u{2cef}', '\u{2cf0}', '\u{2cf1}', '\u{2de0}', '\u{2de1}', '\u{2de2}', '\u{2de3}', '\u{2de4}', '\u{2de5}', '\u{2de6}',
    '\u{2de7}', '\u{2de8}', '\u{2de9}', '\u{2dea}', '\u{2deb}', '\u{2dec}', '\u{2ded}', '\u{2dee}', '\u{2def}', '\u{2df0}', '\u{2df1}',
    '\u{2df2}', '\u{2df3}', '\u{2df4}', '\u{2df5}', '\u{2df6}', '\u{2df7}', '\u{2df8}', '\u{2df9}', '\u{2dfa}', '\u{2dfb}', '\u{2dfc}',
    '\u{2dfd}', '\u{2dfe}', '\u{2dff}', '\u{a66f}', '\u{a67c}', '\u{a67d}', '\u{a6f0}', '\u{a6f1}', '\u{a8e0}', '\u{a8e1}', '\u{a8e2}',
    '\u{a8e3}', '\u{a8e4}', '\u{a8e5}', '\u{a8e6}', '\u{a8e7}', '\u{a8e8}', '\u{a8e9}', '\u{a8ea}', '\u{a8eb}', '\u{a8ec}', '\u{a8ed}',
    '\u{a8ee}', '\u{a8ef}', '\u{a8f0}', '\u{a8f1}', '\u{aab0}', '\u{aab2}', '\u{aab3}', '\u{aab7}', '\u{aab8}', '\u{aabe}', '\u{aabf}',
    '\u{aac1}', '\u{fe20}', '\u{fe21}', '\u{fe22}', '\u{fe23}', '\u{fe24}', '\u{fe25}', '\u{fe26}', '\u{10a0f}', '\u{10a38}',
    '\u{1d185}', '\u{1d186}', '\u{1d187}', '\u{1d188}', '\u{1d189}', '\u{1d1aa}', '\u{1d1ab}', '\u{1d1ac}', '\u{1d1ad}', '\u{1d242}',
    '\u{1d243}', '\u{1d244}',
];

fn chunks(png: &[u8]) -> Vec<String> {
    let encoded = STANDARD.encode(png);
    encoded.as_bytes().chunks(CHUNK_BYTES).map(|chunk| String::from_utf8_lossy(chunk).into_owned()).collect()
}

fn command(index: usize, chunk: &str, is_last: bool, first_keys: &str) -> String {
    let more = u8::from(!is_last);
    let keys = if index == 0 { format!("{first_keys},m={more}") } else { format!("q=2,m={more}") };
    format!("\x1b_G{keys};{chunk}\x1b\\")
}

fn commands(png: &[u8], first_keys: &str) -> Vec<String> {
    let chunks = chunks(png);
    let last = chunks.len().saturating_sub(1);
    chunks.iter().enumerate().map(|(index, chunk)| command(index, chunk, index == last, first_keys)).collect()
}

fn chunked(png: &[u8], first_keys: &str) -> String {
    commands(png, first_keys).concat()
}

/// Draws the picture at the cursor into its cell box with `C=1`, cursor unmoved: where kitty, Ghostty and WezTerm
/// would otherwise leave it differs.
pub fn draw(picture: &Picture) -> String {
    chunked(&picture.png, &format!("a=T,f=100,q=2,C=1,c={},r={}", picture.cols, picture.rows.max(1)))
}

/// An image id a 256-colour placeholder can carry, the low byte as its colour and the high byte as a third diacritic,
/// so it survives a tmux that downgrades true colour. Taken from the PNG: the same diagram shares one image, and
/// pictures left in the scrollback keep theirs.
pub fn placeholder_id(png: &[u8]) -> u32 {
    let hash = png.iter().fold(0x811c_9dc5_u32, |hash, &byte| (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193));
    let low = hash % 255 + 1;
    let high = (hash >> 8) & 0xff;
    (high << 24) | low
}

/// Sends the picture through tmux under `id` with a virtual placement of its cell box, for placeholder cells to show.
pub fn store_through_tmux(picture: &Picture, id: u32) -> String {
    let keys = format!("a=T,U=1,f=100,i={id},c={},r={},q=2", picture.cols, picture.rows.max(1));
    commands(&picture.png, &keys).iter().map(|command| tmux::passthrough(command)).collect()
}

/// Row `row` of the picture `id` as `cols` placeholder cells: the first names the row, column 0 and the id's high
/// byte, the others follow on. `None` past the last row the diacritics can number.
pub fn placeholder_row(id: u32, row: usize, cols: u16) -> Option<String> {
    let row_mark = DIACRITICS.get(row)?;
    let high_mark = DIACRITICS[(id >> 24) as usize];
    let rest = PLACEHOLDER.to_string().repeat(usize::from(cols.saturating_sub(1)));
    Some(format!("\x1b[38;5;{}m{PLACEHOLDER}{row_mark}{}{high_mark}{rest}\x1b[39m", id & 0xff, DIACRITICS[0]))
}

/// Draws the picture inside tmux: sent once through it, then shown by rows of placeholder cells after `indent`, which
/// tmux keeps, scrolls and redraws like any text.
pub fn draw_through_tmux(picture: &Picture, indent: &str) -> Option<String> {
    let id = placeholder_id(&picture.png);
    let rows = (0..usize::from(picture.rows.max(1))).map(|row| Some(format!("{indent}{}\n", placeholder_row(id, row, picture.cols)?)));
    Some(store_through_tmux(picture, id) + &rows.collect::<Option<String>>()?)
}

/// A part of a PNG, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// A stored picture drawn at the cursor: the `crop` of its PNG scaled into `cols`×`rows` cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub id: u32,
    pub crop: Crop,
    pub cols: u16,
    pub rows: u16,
}

/// Sends the PNG once under `id` without drawing it, so frames can place it again and again.
pub fn store(png: &[u8], id: u32) -> String {
    chunked(png, &format!("a=t,f=100,i={id},q=2"))
}

/// Draws a stored picture at the cursor, leaving the cursor where it was. Placing the same image and placement id
/// again replaces the previous placement.
pub fn place(placement: &Placement) -> String {
    let Placement { id, crop: Crop { x, y, width, height }, cols, rows } = *placement;
    format!("\x1b_Ga=p,i={id},p={PLACEMENT_ID},x={x},y={y},w={width},h={height},c={cols},r={rows},C=1,q=2\x1b\\")
}

/// Removes every placement of the picture but keeps its data, so it can be placed again without resending it.
pub fn hide(id: u32) -> String {
    format!("\x1b_Ga=d,d=i,i={id},q=2\x1b\\")
}

/// Frees one picture, placements and data.
pub fn free(id: u32) -> String {
    format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\")
}

/// Frees every picture with an id in `ids`, placements and data.
pub fn forget(ids: std::ops::RangeInclusive<u32>) -> String {
    format!("\x1b_Ga=d,d=R,x={},y={},q=2\x1b\\", ids.start(), ids.end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(png: Vec<u8>) -> Picture {
        Picture { png, cols: 40, rows: 3, alt: "flow".to_owned(), indent: crate::document::Line::blank() }
    }

    #[test]
    fn a_small_picture_is_one_final_chunk() {
        assert_eq!(draw(&sample(vec![1, 2, 3])), "\x1b_Ga=T,f=100,q=2,C=1,c=40,r=3,m=0;AQID\x1b\\");
    }

    #[test]
    fn a_large_picture_is_split_in_4096_byte_chunks() {
        let out = draw(&sample(vec![0; 3 * 4096]));
        let commands: Vec<&str> = out.split("\x1b_G").skip(1).collect();

        assert_eq!(commands.len(), 4);
        assert!(commands[0].starts_with("a=T,f=100,q=2,C=1,c=40,r=3,m=1;"));
        assert!(commands[1].starts_with("q=2,m=1;"));
        assert!(commands[3].starts_with("q=2,m=0;"));
        let payload = |command: &str| command.split([';', '\x1b']).nth(1).map_or(0, str::len);
        assert_eq!(commands.iter().map(|command| payload(command)).collect::<Vec<_>>(), [4096, 4096, 4096, 4096]);
    }

    fn readable(sequence: &str) -> String {
        sequence.replace('\x1b', "␛")
    }

    #[test]
    fn the_pager_commands_follow_the_kitty_protocol() {
        let placement = Placement { id: 7, crop: Crop { x: 0, y: 40, width: 400, height: 60 }, cols: 40, rows: 3 };
        let commands = [store(&[1, 2, 3], 7), place(&placement), hide(7), forget(7..=9)].map(|command| readable(&command));

        insta::assert_snapshot!(commands.join("\n"));
    }

    #[test]
    fn a_placeholder_row_names_its_row_column_and_id_on_the_first_cell_only() {
        let id = 0x0700_002a;

        assert_eq!(placeholder_row(id, 2, 3), Some("\x1b[38;5;42m\u{10eeee}\u{30e}\u{305}\u{33f}\u{10eeee}\u{10eeee}\x1b[39m".to_owned()));
        assert_eq!(placeholder_row(id, 297, 3), None);
    }

    #[test]
    fn placeholder_ids_fit_a_256_colour_cell_and_follow_the_png() {
        let ids = [placeholder_id(&[1, 2, 3]), placeholder_id(&[1, 2, 4]), placeholder_id(&[])];

        assert!(ids.iter().all(|id| (1..=255).contains(&(id & 0xff)) && id & 0x00ff_ff00 == 0), "{ids:x?}");
        assert_eq!(placeholder_id(&[1, 2, 3]), ids[0]);
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn inside_tmux_every_chunk_passes_through_and_each_row_is_placeholders() {
        let drawn = draw_through_tmux(&sample(vec![0; 4096]), "  ").unwrap_or_default();
        let id = placeholder_id(&[0; 4096]);

        assert_eq!(drawn.matches("\x1bPtmux;\x1b\x1b_G").count(), 2, "{drawn:?}");
        assert!(drawn.contains(&format!("\x1bPtmux;\x1b\x1b_Ga=T,U=1,f=100,i={id},c=40,r=3,q=2,m=1;")));
        assert_eq!(drawn.matches('\n').count(), 3);
        assert!(drawn.ends_with(&format!("  {}\n", placeholder_row(id, 2, 40).unwrap_or_default())));
    }

    #[test]
    fn a_stored_picture_is_chunked_with_its_keys_on_the_first_chunk_only() {
        let out = store(&vec![0; 3 * 4096], 7);
        let commands: Vec<&str> = out.split("\x1b_G").skip(1).collect();

        assert_eq!(commands.len(), 4);
        assert!(commands[0].starts_with("a=t,f=100,i=7,q=2,m=1;"));
        assert!(commands[1].starts_with("q=2,m=1;"));
        assert!(commands[3].starts_with("q=2,m=0;"));
    }
}
