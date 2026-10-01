use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::document::Picture;

const CHUNK_BYTES: usize = 4096;
const PLACEMENT_ID: u32 = 1;

fn chunks(png: &[u8]) -> Vec<String> {
    let encoded = STANDARD.encode(png);
    encoded.as_bytes().chunks(CHUNK_BYTES).map(|chunk| String::from_utf8_lossy(chunk).into_owned()).collect()
}

fn command(index: usize, chunk: &str, is_last: bool, first_keys: &str) -> String {
    let more = u8::from(!is_last);
    let keys = if index == 0 { format!("{first_keys},m={more}") } else { format!("q=2,m={more}") };
    format!("\x1b_G{keys};{chunk}\x1b\\")
}

fn chunked(png: &[u8], first_keys: &str) -> String {
    let chunks = chunks(png);
    let last = chunks.len().saturating_sub(1);
    chunks.iter().enumerate().map(|(index, chunk)| command(index, chunk, index == last, first_keys)).collect()
}

/// Draws the picture at the cursor into its cell box with `C=1`, cursor unmoved: where kitty, Ghostty and WezTerm
/// would otherwise leave it differs.
pub fn draw(picture: &Picture) -> String {
    chunked(&picture.png, &format!("a=T,f=100,q=2,C=1,c={},r={}", picture.cols, picture.rows.max(1)))
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
    fn a_stored_picture_is_chunked_with_its_keys_on_the_first_chunk_only() {
        let out = store(&vec![0; 3 * 4096], 7);
        let commands: Vec<&str> = out.split("\x1b_G").skip(1).collect();

        assert_eq!(commands.len(), 4);
        assert!(commands[0].starts_with("a=t,f=100,i=7,q=2,m=1;"));
        assert!(commands[1].starts_with("q=2,m=1;"));
        assert!(commands[3].starts_with("q=2,m=0;"));
    }
}
