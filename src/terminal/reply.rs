use crate::document::Rgb;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const KITTY_QUERY_ID: &str = "i=31";

/// What the terminal answered to one round of queries; anything missing or malformed reads as "unsupported".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Replies {
    pub graphics: bool,
    pub background: Option<Rgb>,
    /// The DA1 reply arrived, so every earlier reply has too.
    pub answered: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Sequence<'a> {
    Csi(&'a [u8]),
    Osc(&'a [u8]),
    Apc(&'a [u8]),
}

fn is_printable(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| (0x20..=0x7e).contains(byte))
}

fn string_end(bytes: &[u8], accepts_bel: bool) -> Option<(usize, usize)> {
    bytes.iter().enumerate().find_map(|(index, &byte)| match byte {
        BEL if accepts_bel => Some((index, index + 1)),
        ESC if bytes.get(index + 1) == Some(&b'\\') => Some((index, index + 2)),
        _ => None,
    })
}

fn string_sequence(bytes: &[u8], accepts_bel: bool) -> Option<(&[u8], usize)> {
    let body = bytes.get(2..)?;
    let (end, consumed) = string_end(body, accepts_bel)?;
    let content = &body[..end];
    is_printable(content).then_some((content, consumed + 2))
}

fn csi_sequence(bytes: &[u8]) -> Option<(&[u8], usize)> {
    let body = bytes.get(2..)?;
    let final_index = body.iter().position(|byte| !(0x20..=0x3f).contains(byte))?;
    let is_final = (0x40..=0x7e).contains(&body[final_index]);
    is_final.then_some((&body[..=final_index], final_index + 3))
}

fn next_sequence(bytes: &[u8]) -> Option<(Sequence<'_>, usize)> {
    if bytes.first() != Some(&ESC) {
        return None;
    }

    match bytes.get(1)? {
        b'[' => csi_sequence(bytes).map(|(body, consumed)| (Sequence::Csi(body), consumed)),
        b']' => string_sequence(bytes, true).map(|(body, consumed)| (Sequence::Osc(body), consumed)),
        b'_' => string_sequence(bytes, false).map(|(body, consumed)| (Sequence::Apc(body), consumed)),
        _ => None,
    }
}

fn sequences(bytes: &[u8]) -> Vec<Sequence<'_>> {
    let mut found = Vec::new();
    let mut rest = bytes;
    while let Some((sequence, consumed)) = next_sequence(rest) {
        found.push(sequence);
        rest = &rest[consumed..];
    }
    found
}

fn is_primary_attributes(body: &[u8]) -> bool {
    body.first() == Some(&b'?') && body.last() == Some(&b'c')
}

fn is_graphics_ok(body: &[u8]) -> bool {
    let text = String::from_utf8_lossy(body);
    let Some((keys, message)) = text.strip_prefix('G').and_then(|rest| rest.split_once(';')) else { return false };
    keys.split(',').any(|key| key == KITTY_QUERY_ID) && message == "OK"
}

fn channel(hex: &str) -> Option<u8> {
    let is_valid = (1..=4).contains(&hex.len()) && hex.chars().all(|character| character.is_ascii_hexdigit());
    let value = u32::from_str_radix(hex, 16).ok().filter(|_| is_valid)?;
    let max = 16u32.pow(hex.len() as u32) - 1;
    Some((value * 255 / max) as u8)
}

fn background(body: &[u8]) -> Option<Rgb> {
    let text = std::str::from_utf8(body).ok()?;
    let channels: Vec<&str> = text.strip_prefix("11;rgb:")?.split('/').collect();
    let [red, green, blue] = channels.as_slice() else { return None };
    Some(Rgb(channel(red)?, channel(green)?, channel(blue)?))
}

fn record(replies: Replies, sequence: &Sequence<'_>) -> Replies {
    match sequence {
        Sequence::Csi(body) if is_primary_attributes(body) => Replies { answered: true, ..replies },
        Sequence::Apc(body) if is_graphics_ok(body) => Replies { graphics: true, ..replies },
        Sequence::Osc(body) => Replies { background: background(body).or(replies.background), ..replies },
        _ => replies,
    }
}

/// Reads the replies from the start of `bytes`, stopping at the first byte that is not a well-formed reply.
pub fn parse(bytes: &[u8]) -> Replies {
    sequences(bytes).iter().fold(Replies::default(), record)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DA1: &[u8] = b"\x1b[?62;22;52c";

    #[test]
    fn nothing_means_nothing_supported() {
        assert_eq!(parse(b""), Replies::default());
    }

    #[test]
    fn the_da1_reply_marks_the_round_complete() {
        assert_eq!(parse(DA1), Replies { answered: true, ..Replies::default() });
    }

    #[test]
    fn a_kitty_ok_then_da1_means_graphics() {
        let replies = parse(&[b"\x1b_Gi=31;OK\x1b\\".as_slice(), DA1].concat());

        assert_eq!(replies, Replies { graphics: true, background: None, answered: true });
    }

    #[test]
    fn a_kitty_error_is_no_graphics() {
        assert!(!parse(b"\x1b_Gi=31;ENOTSUPPORTED:x\x1b\\").graphics);
        assert!(!parse(b"\x1b_Gi=7;OK\x1b\\").graphics);
    }

    #[test]
    fn a_background_reply_parses_with_either_terminator() {
        let dark = Some(Rgb(0x1e, 0x1e, 0x2e));

        assert_eq!(parse(b"\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\").background, dark);
        assert_eq!(parse(b"\x1b]11;rgb:1e1e/1e1e/2e2e\x07").background, dark);
    }

    #[test]
    fn background_channels_of_any_width_scale_to_eight_bits() {
        assert_eq!(parse(b"\x1b]11;rgb:f/80/fff\x07").background, Some(Rgb(255, 128, 255)));
    }

    #[test]
    fn a_malformed_background_is_ignored() {
        for reply in
            [b"\x1b]11;rgb:zz/00/00\x07".as_slice(), b"\x1b]11;rgb:00/00\x07", b"\x1b]11;rgb:00000/0/0\x07", b"\x1b]10;rgb:0/0/0\x07"]
        {
            assert_eq!(parse(reply).background, None, "{reply:?}");
        }
    }

    #[test]
    fn all_three_replies_in_one_buffer() {
        let replies = parse(&[b"\x1b_Gi=31;OK\x1b\\".as_slice(), b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\", DA1].concat());

        assert_eq!(replies, Replies { graphics: true, background: Some(Rgb(255, 255, 255)), answered: true });
    }

    #[test]
    fn parsing_stops_at_unexpected_bytes() {
        let replies = parse(&[b"\x1b_Gi=31;OK\x1b\\".as_slice(), b"garbage", DA1].concat());

        assert_eq!(replies, Replies { graphics: true, background: None, answered: false });
    }

    #[test]
    fn an_unterminated_reply_is_not_read() {
        assert_eq!(parse(b"\x1b]11;rgb:ffff/ffff/ffff"), Replies::default());
        assert_eq!(parse(b"\x1b_Gi=31;OK"), Replies::default());
        assert_eq!(parse(b"\x1b[?62"), Replies::default());
    }

    #[test]
    fn a_control_byte_inside_a_reply_rejects_it() {
        assert_eq!(parse(b"\x1b_Gi=31;O\x00K\x1b\\"), Replies::default());
    }
}
