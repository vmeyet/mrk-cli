use std::borrow::Cow;

/// Lines whose `#123;` is a colour, never an entity, as Mermaid itself reads them.
const STYLING: [&str; 4] = ["style ", "classDef ", "linkStyle ", "%%"];
const ENTITIES: [(&str, char); 6] = [("amp", '&'), ("lt", '<'), ("gt", '>'), ("quot", '"'), ("apos", '\''), ("nbsp", '\u{a0}')];
/// The longest reference, `#x10FFFF`.
const MAX_REFERENCE_BYTES: usize = 8;
/// The one line break both engines read.
const BREAK: &str = "<br>";

/// The source with its labels written as Mermaid shows them, for both engines: HTML tags dropped, a `<br>` in any
/// spelling made the one both read, a markdown string's backticks and emphasis markers dropped, entities decoded in
/// quoted strings, where no decoded `<` or `"` can end a label early. Never longer than the source, in one pass.
pub fn clean(source: &str) -> String {
    source.split_inclusive('\n').map(cleaned_line).collect()
}

fn cleaned_line(line: &str) -> Cow<'_, str> {
    if is_styling(line) {
        return Cow::Borrowed(line);
    }
    Cow::Owned(with_strings_cleaned(&without_tags(line)).into_owned())
}

fn is_styling(line: &str) -> bool {
    let line = line.trim_start();
    STYLING.iter().any(|keyword| line.starts_with(keyword))
}

struct Tag<'a> {
    name: &'a str,
}

impl Tag<'_> {
    fn shown(&self) -> &'static str {
        if self.name.eq_ignore_ascii_case("br") { BREAK } else { "" }
    }
}

fn without_tags(line: &str) -> Cow<'_, str> {
    if !line.contains('<') {
        return Cow::Borrowed(line);
    }
    let mut kept = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find('<') {
        kept.push_str(&rest[..at]);
        let (shown, length) = tag_at(&rest[at..]).map_or_else(|| angles_at(&rest[at..]), |(tag, length)| (tag.shown(), length));
        kept.push_str(shown);
        rest = &rest[at + length..];
    }
    kept.push_str(rest);
    Cow::Owned(kept)
}

/// `<name>`, `</name>`, `<name/>` or `<name attributes>` on one line; an attribute holding `<` is no tag, so each
/// look ahead stops at the next candidate and the scan stays linear.
fn tag_at(text: &str) -> Option<(Tag<'_>, usize)> {
    let opened = text.strip_prefix('<')?;
    let named = opened.strip_prefix('/').unwrap_or(opened);
    let name = &named[..named.find(|character: char| !character.is_ascii_alphanumeric()).unwrap_or(named.len())];
    let after = &named[name.len()..];
    if !name.starts_with(|character: char| character.is_ascii_alphabetic()) || !after.starts_with(['>', '/', ' ', '\t']) {
        return None;
    }
    let end = opened.find(['<', '>', '\n']).filter(|&end| opened[end..].starts_with('>'))?;
    Some((Tag { name }, end + 2))
}

/// A run of `<` kept whole, so the `<interface>` of a class annotation `<<interface>>` is never read as a tag.
fn angles_at(text: &str) -> (&str, usize) {
    let length = text.len() - text.trim_start_matches('<').len();
    (&text[..length], length)
}

fn with_strings_cleaned(line: &str) -> Cow<'_, str> {
    if !line.contains('"') {
        return Cow::Borrowed(line);
    }
    let parts: Vec<&str> = line.split('"').collect();
    let is_unclosed = |index: usize| parts.len().is_multiple_of(2) && index == parts.len() - 1;
    let cleaned = parts.iter().enumerate().map(|(index, part)| {
        if index.is_multiple_of(2) {
            Cow::Borrowed(*part)
        } else if is_unclosed(index) {
            Cow::Owned(format!("\"{part}"))
        } else {
            Cow::Owned(string(part, parts[index - 1].ends_with('|') && parts[index + 1].starts_with('|')))
        }
    });
    Cow::Owned(cleaned.collect())
}

/// A quoted string, its quotes dropped between the pipes of an edge label where the picture engine would keep them.
fn string(text: &str, is_edge_label: bool) -> String {
    let words = without_markdown(text);
    let text = decoded(&words);
    if is_edge_label && !text.contains(['|', '"']) { text.into_owned() } else { format!("\"{text}\"") }
}

/// A markdown string, `` `**bold** _italic_` ``, as its words.
fn without_markdown(text: &str) -> Cow<'_, str> {
    let Some(markdown) = text.strip_prefix('`').and_then(|text| text.strip_suffix('`')) else { return Cow::Borrowed(text) };
    let characters: Vec<char> = markdown.chars().collect();
    Cow::Owned((0..characters.len()).filter(|&index| !is_emphasis(&characters, index)).map(|index| characters[index]).collect())
}

/// Every `*`, and a `_` unless it joins two parts of a word, as in `snake_case`.
fn is_emphasis(characters: &[char], index: usize) -> bool {
    let is_word = |at: Option<usize>| at.and_then(|at| characters.get(at)).is_some_and(|character| character.is_alphanumeric());
    match characters[index] {
        '*' => true,
        '_' => !(is_word(index.checked_sub(1)) && is_word(Some(index + 1))),
        _ => false,
    }
}

fn decoded(text: &str) -> Cow<'_, str> {
    if !text.contains(['&', '#']) {
        return Cow::Borrowed(text);
    }
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(['&', '#']) {
        kept.push_str(&rest[..at]);
        let (character, length) = entity_at(&rest[at..]).unwrap_or((char::from(rest.as_bytes()[at]), 1));
        kept.push(character);
        rest = &rest[at + length..];
    }
    kept.push_str(rest);
    Cow::Owned(kept)
}

/// `&amp;`, `&#38;` and `&#x26;` as HTML writes them, `#amp;` and `#38;` as Mermaid does; a control character stays
/// encoded.
fn entity_at(text: &str) -> Option<(char, usize)> {
    let (opening, body) = text.split_at(1);
    let end = body.bytes().take(MAX_REFERENCE_BYTES + 1).position(|byte| byte == b';')?;
    let reference = &body[..end];
    let reference = if opening == "&" { reference.strip_prefix('#').unwrap_or(reference) } else { reference };
    let character =
        ENTITIES.iter().find(|(name, _)| *name == reference).map(|(_, character)| *character).or_else(|| numbered(reference))?;
    Some((character, end + 2))
}

fn numbered(reference: &str) -> Option<char> {
    let code = match reference.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => reference.parse(),
    };
    code.ok().and_then(char::from_u32).filter(|character| !character.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_tags_are_dropped_and_their_text_kept() {
        assert_eq!(clean(r#"A["<b>A. Storage</b>"] --> B[<i>B</i> <span class="x">C</span>]"#), r#"A["A. Storage"] --> B[B C]"#);
    }

    #[test]
    fn every_spelling_of_a_line_break_becomes_the_one_both_engines_read() {
        assert_eq!(clean("A[a<br/>b<BR />c<br>d<br\t/>e</br>f]"), "A[a<br>b<br>c<br>d<br>e<br>f]");
    }

    #[test]
    fn any_other_tag_is_dropped() {
        assert_eq!(clean(r#"A["<script>x</script><img src='y.png'/>z<foo bar>"]"#), r#"A["xz"]"#);
    }

    #[test]
    fn mermaid_syntax_around_angles_is_kept() {
        let sources = [
            "classDiagram\n  class Shape {\n    <<interface>>\n  }\n",
            "flowchart LR\n  A <--> B\n  C<-->D\n  E -.-> F\n",
            "sequenceDiagram\n  A->>B: x < y and 2 > 1\n  A<<->>B: both\n",
            "stateDiagram-v2\n  state fork <<fork>>\n",
            "flowchart LR\n  a[if x<y-->z]\n",
        ];

        for source in sources {
            assert_eq!(clean(source), source);
        }
    }

    #[test]
    fn a_markdown_string_loses_its_backticks_and_markers() {
        assert_eq!(clean(r#"A["`**bold** _it_ *em* snake_case`"]"#), r#"A["bold it em snake_case"]"#);
    }

    #[test]
    fn backticks_inside_a_plain_string_are_kept() {
        assert_eq!(clean(r#"A["run `make` now"]"#), r#"A["run `make` now"]"#);
    }

    #[test]
    fn entities_are_decoded_in_quoted_strings_only() {
        let source = r#"A["a &amp; b &lt;T&gt; #quot;q#quot; &#9829; #9829; &#x2665; &nbsp;"] -- a &gt; b --> B"#;

        assert_eq!(clean(source), "A[\"a & b <T> \"q\" ♥ ♥ ♥ \u{a0}\"] -- a &gt; b --> B");
    }

    #[test]
    fn a_decoded_tag_stays_text() {
        assert_eq!(clean(r#"A["&lt;b&gt;x&lt;/b&gt;"]"#), r#"A["<b>x</b>"]"#);
    }

    #[test]
    fn unknown_control_and_malformed_references_stay_as_written() {
        let source = r#"A["&bogus; #1; &#27; &#x; # 12; a&b #12345678901;"]"#;

        assert_eq!(clean(source), source);
    }

    #[test]
    fn styling_lines_keep_their_colours() {
        let source = "flowchart LR\n  style a fill:#123;\n  classDef x fill:#123;\n  linkStyle 0 stroke:#123;\n  %% \"#123;\"\n";

        assert_eq!(clean(source), source);
    }

    #[test]
    fn an_edge_label_loses_its_quotes_unless_they_protect_a_pipe() {
        assert_eq!(clean(r#"A -->|"yes"| B -->|"a|b"| C"#), r#"A -->|yes| B -->|"a|b"| C"#);
    }

    #[test]
    fn an_unclosed_quote_is_kept() {
        assert_eq!(clean(r#"A["x"] --> B["y"#), r#"A["x"] --> B["y"#);
    }

    #[test]
    fn the_cleaned_source_is_never_longer() {
        let hostile = [
            "<a".repeat(32 * 1024),
            "\"`".repeat(32 * 1024),
            "\"&#".repeat(20 * 1024),
            "<<br/>".repeat(10 * 1024),
            format!("\"{}\"", "#x10FFFF;".repeat(7 * 1024)),
        ];

        for source in &hostile {
            assert!(clean(source).len() <= source.len(), "{:?}", &source[..12]);
        }
    }
}
