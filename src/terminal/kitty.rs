use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::ansi::MARGIN;
use super::sanitize;
use crate::document::Picture;

const CHUNK_BYTES: usize = 4096;

fn chunks(png: &[u8]) -> Vec<String> {
    let encoded = STANDARD.encode(png);
    encoded.as_bytes().chunks(CHUNK_BYTES).map(|chunk| String::from_utf8_lossy(chunk).into_owned()).collect()
}

fn command(index: usize, chunk: &str, is_last: bool, picture: &Picture) -> String {
    let more = u8::from(!is_last);
    let keys = if index == 0 {
        format!("a=T,f=100,q=2,C=1,c={},r={},m={more}", picture.cols, picture.rows.max(1))
    } else {
        format!("q=2,m={more}")
    };
    format!("\x1b_G{keys};{chunk}\x1b\\")
}

fn transmit(picture: &Picture) -> String {
    let chunks = chunks(&picture.png);
    let last = chunks.len().saturating_sub(1);
    chunks.iter().enumerate().map(|(index, chunk)| command(index, chunk, index == last, picture)).collect()
}

/// Draws the picture with the kitty graphics protocol behind the margin, then leaves the cursor at the start of the line below it.
/// The rows are scrolled into view first and the picture is drawn with `C=1` (cursor unmoved): where kitty, Ghostty and
/// WezTerm would otherwise leave the cursor differs, and drawing at the bottom of the screen must not clip the picture.
pub fn picture(picture: &Picture) -> String {
    let rows = picture.rows.max(1);
    let reserve = "\n".repeat(usize::from(rows));
    format!("{reserve}\x1b[{rows}A\r{MARGIN}{}\x1b[{rows}B\r", transmit(picture))
}

/// What stands in for a picture when the terminal cannot draw it.
pub fn placeholder(picture: &Picture) -> String {
    format!("{MARGIN}[picture: {}]\n", sanitize::text(&picture.alt))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(png: Vec<u8>) -> Picture {
        Picture { png, cols: 40, rows: 3, alt: "flow".to_owned() }
    }

    #[test]
    fn a_small_picture_is_one_final_chunk() {
        let out = picture(&sample(vec![1, 2, 3]));

        assert_eq!(out, "\n\n\n\x1b[3A\r  \x1b_Ga=T,f=100,q=2,C=1,c=40,r=3,m=0;AQID\x1b\\\x1b[3B\r");
    }

    #[test]
    fn a_large_picture_is_split_in_4096_byte_chunks() {
        let out = picture(&sample(vec![0; 3 * 4096]));
        let commands: Vec<&str> = out.split("\x1b_G").skip(1).collect();

        assert_eq!(commands.len(), 4);
        assert!(commands[0].starts_with("a=T,f=100,q=2,C=1,c=40,r=3,m=1;"));
        assert!(commands[1].starts_with("q=2,m=1;"));
        assert!(commands[3].starts_with("q=2,m=0;"));
        let payload = |command: &str| command.split([';', '\x1b']).nth(1).map_or(0, str::len);
        assert_eq!(commands.iter().map(|command| payload(command)).collect::<Vec<_>>(), [4096, 4096, 4096, 4096]);
    }

    #[test]
    fn the_placeholder_sanitizes_the_alt_text() {
        assert_eq!(placeholder(&sample(vec![])).as_str(), "  [picture: flow]\n");
        assert_eq!(placeholder(&Picture { alt: "a\x1b]0;x\x07".to_owned(), ..sample(vec![]) }), "  [picture: a]0;x]\n");
    }
}
