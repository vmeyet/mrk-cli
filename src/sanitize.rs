//! Text and links made safe to write to a terminal, which interprets the escape sequences an untrusted input can carry.
use crate::text::{expand_tabs, is_forbidden};

const MAX_LINK_BYTES: usize = 2048;
const WEB_SCHEMES: [&str; 3] = ["http", "https", "mailto"];
/// Each ends on the slash that starts the path, so a `file:` link names no host but this machine.
const LOCAL_FILE_PREFIXES: [&str; 2] = ["file:///", "file://localhost/"];

/// The text with every character a terminal could interpret removed: C0 and C1 controls, DEL, bidi marks, overrides
/// and isolates, and the zero-width space. A tab becomes spaces.
pub fn text(raw: &str) -> String {
    clean(raw, false)
}

/// `text` for a message of several lines: the newlines are kept.
pub fn lines(raw: &str) -> String {
    clean(raw, true)
}

fn clean(raw: &str, keeps_newlines: bool) -> String {
    let is_kept = |character: char| (keeps_newlines && character == '\n') || !is_forbidden(character);
    expand_tabs(raw).chars().filter(|&character| is_kept(character)).collect()
}

fn percent_encode(raw: &str) -> String {
    raw.bytes().map(|byte| if byte.is_ascii_graphic() { char::from(byte).to_string() } else { format!("%{byte:02X}") }).collect()
}

fn percent_decode(raw: &str) -> Vec<u8> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let escaped = bytes.get(index + 1..index + 3).filter(|_| byte == b'%').and_then(hex_byte);
        decoded.push(escaped.unwrap_or(byte));
        index += if escaped.is_some() { 3 } else { 1 };
    }
    decoded
}

fn hex_byte(digits: &[u8]) -> Option<u8> {
    let digits = std::str::from_utf8(digits).ok().filter(|digits| digits.bytes().all(|digit| digit.is_ascii_hexdigit()))?;
    u8::from_str_radix(digits, 16).ok()
}

fn scheme(target: &str) -> Option<&str> {
    let (scheme, _) = target.split_once(':')?;
    let starts_with_letter = scheme.starts_with(|character: char| character.is_ascii_alphabetic());
    let is_valid = starts_with_letter && scheme.chars().all(|character| character.is_ascii_alphanumeric() || "+-.".contains(character));
    is_valid.then_some(scheme)
}

/// A `file:` link to another host (`file://host/share`, `file:////host/share`, `file:///\\host\share`) opens an SMB share
/// on some systems, which sends it the user's credentials.
fn is_local_file(target: &str) -> bool {
    let path = LOCAL_FILE_PREFIXES.iter().find_map(|prefix| {
        let head = target.get(..prefix.len())?;
        head.eq_ignore_ascii_case(prefix).then(|| &target[prefix.len()..])
    });
    path.is_some_and(|path| is_plain_path(&percent_decode(path)))
}

/// Openers decode the path and resolve `.` and `..` before reading it, so `/a/..//host/share` reaches a share:
/// decoded once, the path must hold no backslash and only named segments, a trailing slash aside.
fn is_plain_path(path: &[u8]) -> bool {
    let mut segments = path.strip_suffix(b"/").unwrap_or(path).split(|&byte| byte == b'/');
    let has_only_names = path.is_empty() || segments.all(|segment| !matches!(segment, b"" | b"." | b".."));
    has_only_names && !path.contains(&b'\\')
}

fn is_allowed(target: &str) -> bool {
    match scheme(target) {
        Some(scheme) if scheme.eq_ignore_ascii_case("file") => is_local_file(target),
        Some(scheme) => WEB_SCHEMES.iter().any(|allowed| scheme.eq_ignore_ascii_case(allowed)),
        None => false,
    }
}

/// The OSC 8 target for a link, or `None` when the link must not be clickable: longer than 2,048 bytes,
/// with a scheme other than http, https, mailto and file (so relative paths and `javascript:` are dropped),
/// or a `file:` link to another host.
pub fn link(raw: &str) -> Option<String> {
    let target = percent_encode(&text(raw));
    let is_short = target.len() <= MAX_LINK_BYTES;
    (is_short && is_allowed(&target)).then_some(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(text("Hello, wörld 漢字 ✔"), "Hello, wörld 漢字 ✔");
    }

    #[test]
    fn escape_and_bell_are_removed() {
        assert_eq!(text("a\x1bb\x07c"), "abc");
    }

    #[test]
    fn a_csi_sequence_loses_its_escape_and_cannot_style() {
        assert_eq!(text("\x1b[31mred\x1b[0m"), "[31mred[0m");
    }

    #[test]
    fn an_osc_52_clipboard_write_is_defused() {
        let clean = text("\x1b]52;c;ZWNobyBwd25lZA==\x07after");

        assert!(!clean.contains('\x1b') && !clean.contains('\x07'));
        assert_eq!(clean, "]52;c;ZWNobyBwd25lZA==after");
    }

    #[test]
    fn a_retitle_ended_by_string_terminator_is_defused() {
        assert_eq!(text("\x1b]0;pwned\x1b\\x"), "]0;pwned\\x");
    }

    #[test]
    fn c1_controls_are_removed() {
        assert_eq!(text("a\u{9b}31mb\u{9d}0;t\u{9c}c\u{80}\u{9f}"), "a31mb0;tc");
    }

    #[test]
    fn delete_and_other_c0_controls_are_removed() {
        assert_eq!(text("a\x7fb\x00c\x08d\re\nf\x0bg"), "abcdefg");
    }

    #[test]
    fn bidi_overrides_and_isolates_are_removed() {
        assert_eq!(text("a\u{202a}b\u{202b}c\u{202c}d\u{202d}e\u{202e}f\u{2066}g\u{2067}h\u{2068}i\u{2069}j"), "abcdefghij");
    }

    #[test]
    fn bidi_marks_and_the_zero_width_space_are_removed() {
        assert_eq!(text("a\u{200b}b\u{200e}c\u{200f}d\u{061c}e"), "abcde");
    }

    #[test]
    fn joiners_and_other_format_characters_are_kept() {
        assert_eq!(text("a\u{200c}b\u{200d}c\u{00ad}d"), "a\u{200c}b\u{200d}c\u{00ad}d");
    }

    #[test]
    fn a_tab_becomes_spaces() {
        assert_eq!(text("a\tb"), "a    b");
    }

    #[test]
    fn lines_keep_their_newlines_and_lose_everything_else() {
        assert_eq!(lines("a\x1b]0;t\x07\r\nb\u{9b}\n"), "a]0;t\nb\n");
    }

    #[test]
    fn web_mail_and_file_links_pass() {
        for target in ["https://example.com/a?b=c#d", "http://x.y", "mailto:me@example.com", "file:///tmp/a.md", "HTTPS://EXAMPLE.COM"] {
            assert_eq!(link(target).as_deref(), Some(target), "{target}");
        }
    }

    #[test]
    fn file_links_pass_only_for_this_machine() {
        for target in ["file:///C:/notes.md", "file://localhost/tmp/a.md", "FILE://LocalHost/tmp/a.md"] {
            assert_eq!(link(target).as_deref(), Some(target), "{target}");
        }
        for target in ["file://evil.host/share/x", "file:////evil.host/share", "file:///\\\\evil.host\\share", "file:/tmp/a.md", "file:"] {
            assert_eq!(link(target), None, "{target}");
        }
    }

    #[test]
    fn an_encoded_slash_or_backslash_cannot_name_another_host() {
        for target in ["file:///%5C%5Cevil.host%5Cshare", "file:///%2f%2Fevil.host/x", "file:///tmp/a%5cb"] {
            assert_eq!(link(target), None, "{target}");
        }
    }

    #[test]
    fn a_dot_or_empty_segment_cannot_climb_to_another_host() {
        for target in
            ["file:///.//host/share/x", "file:///%2e%2e//host/share/x", "file:///a/..//host/share/x", "file:///a//b", "file:///a/./b"]
        {
            assert_eq!(link(target), None, "{target}");
        }
    }

    #[test]
    fn a_plain_path_with_dots_in_names_or_a_trailing_slash_passes() {
        for target in ["file:///", "file:///tmp/", "file:///tmp/.config/a.b.md", "file:///tmp/a%20b"] {
            assert_eq!(link(target).as_deref(), Some(target), "{target}");
        }
    }

    #[test]
    fn a_bad_escape_is_kept_as_written() {
        assert_eq!(percent_decode("a%zz%4"), b"a%zz%4");
        assert_eq!(percent_decode("%2e%2E%+1"), b"..%+1");
    }

    #[test]
    fn every_tab_becomes_spaces_wherever_it_sits() {
        assert_eq!(text("\ta\x1b\tb\t"), "    a    b    ");
    }

    #[test]
    fn javascript_and_unknown_schemes_are_dropped() {
        for target in ["javascript:alert(1)", "JavaScript:alert(1)", "data:text/html,x", "vbscript:x", "ssh://host", "x-man-page://ls"] {
            assert_eq!(link(target), None, "{target}");
        }
    }

    #[test]
    fn links_without_a_scheme_are_dropped() {
        for target in ["docs/readme.md", "#section", "//example.com", "", ":nothing", "1http://x"] {
            assert_eq!(link(target), None, "{target:?}");
        }
    }

    #[test]
    fn a_leading_space_does_not_smuggle_a_scheme() {
        assert_eq!(link(" javascript:alert(1)"), None);
    }

    #[test]
    fn a_link_cannot_close_the_hyperlink_or_inject_a_sequence() {
        assert_eq!(link("https://x.y/\x1b\\\x1b]52;c;AAAA\x07").as_deref(), Some("https://x.y/\\]52;c;AAAA"));
    }

    #[test]
    fn spaces_and_non_ascii_are_percent_encoded() {
        assert_eq!(link("https://x.y/a b/é").as_deref(), Some("https://x.y/a%20b/%C3%A9"));
    }

    #[test]
    fn a_link_at_the_cap_passes_and_one_byte_over_is_dropped() {
        let prefix = "https://x.y/";
        let at_cap = format!("{prefix}{}", "a".repeat(MAX_LINK_BYTES - prefix.len()));
        let over_cap = format!("{at_cap}a");

        assert_eq!(link(&at_cap).as_deref(), Some(at_cap.as_str()));
        assert_eq!(link(&over_cap), None);
    }

    #[test]
    fn percent_encoding_counts_toward_the_cap() {
        let long = format!("https://x.y/{}", "é".repeat(400));

        assert_eq!(link(&long), None);
    }
}
