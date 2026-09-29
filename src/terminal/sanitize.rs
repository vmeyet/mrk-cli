const TAB: &str = "    ";
const MAX_LINK_BYTES: usize = 2048;
const ALLOWED_SCHEMES: [&str; 4] = ["http", "https", "mailto", "file"];

fn is_bidi_control(character: char) -> bool {
    matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

fn is_forbidden(character: char) -> bool {
    character.is_control() || is_bidi_control(character)
}

/// The text with every character a terminal could interpret removed: C0 and C1 controls, DEL, bidi overrides and isolates.
/// A tab becomes spaces.
pub fn text(raw: &str) -> String {
    raw.split('\t').map(|part| part.chars().filter(|&character| !is_forbidden(character)).collect::<String>()).collect::<Vec<_>>().join(TAB)
}

/// `text` for a message of several lines: each line is cleaned, the newlines between them are kept.
pub fn lines(raw: &str) -> String {
    raw.split('\n').map(text).collect::<Vec<_>>().join("\n")
}

fn percent_encode(raw: &str) -> String {
    raw.bytes().map(|byte| if byte.is_ascii_graphic() { char::from(byte).to_string() } else { format!("%{byte:02X}") }).collect()
}

fn scheme(target: &str) -> Option<&str> {
    let (scheme, _) = target.split_once(':')?;
    let starts_with_letter = scheme.starts_with(|character: char| character.is_ascii_alphabetic());
    let is_valid = starts_with_letter && scheme.chars().all(|character| character.is_ascii_alphanumeric() || "+-.".contains(character));
    is_valid.then_some(scheme)
}

fn has_allowed_scheme(target: &str) -> bool {
    scheme(target).is_some_and(|scheme| ALLOWED_SCHEMES.iter().any(|allowed| scheme.eq_ignore_ascii_case(allowed)))
}

/// The OSC 8 target for a link, or `None` when the link must not be clickable: longer than 2,048 bytes,
/// or with a scheme other than http, https, mailto and file (so relative paths and `javascript:` are dropped).
pub fn link(raw: &str) -> Option<String> {
    let target = percent_encode(&text(raw));
    let is_short = target.len() <= MAX_LINK_BYTES;
    (is_short && has_allowed_scheme(&target)).then_some(target)
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
    fn other_format_characters_are_kept() {
        assert_eq!(text("a\u{200d}b\u{00ad}c"), "a\u{200d}b\u{00ad}c");
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
