use std::collections::HashMap;

use super::language;

/// syntect's time on a line can grow with the square of its length (a JavaScript regex literal: 8 KB in 1.2 s,
/// 32 KB in 15 s), so a block is highlighted only up to its first longer line.
const MAX_LINE_BYTES: usize = 512;
/// The code one document highlights, every block counted in reading order: past it, blocks are plain.
const MAX_DOCUMENT_BYTES: usize = 256 * 1024;

/// How much of the start of `code` is highlighted, what highlighting it costs: up to its first line over
/// `MAX_LINE_BYTES`, nothing when no syntax is found for its language.
pub(crate) fn highlighted_len(code: &str, language: Option<&str>) -> usize {
    if language.and_then(language::find).is_none() {
        return 0;
    }
    let lines = code.split_inclusive('\n');
    lines.take_while(|line| line.trim_end_matches(['\n', '\r']).len() <= MAX_LINE_BYTES).map(str::len).sum()
}

/// The code blocks of one document its budget lets highlight, each that fits what is left of it in reading order,
/// known by a key unique to the block (the line it starts on).
pub(crate) struct Allowance(HashMap<usize, usize>);

impl Allowance {
    /// `blocks` are each block's key and `highlighted_len`, in reading order.
    pub(crate) fn new(blocks: impl IntoIterator<Item = (usize, usize)>) -> Self {
        let mut admitted = HashMap::new();
        let mut left = MAX_DOCUMENT_BYTES;
        for (key, len) in blocks {
            if len <= left {
                left -= len;
                admitted.insert(key, len);
            }
        }
        Self(admitted)
    }

    /// How much of the block's start is highlighted: its `highlighted_len` when admitted, else nothing.
    pub(crate) fn highlighted(&self, key: usize) -> usize {
        self.0.get(&key).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_naming_a_syntax_is_highlighted_whole() {
        assert_eq!(highlighted_len("let a = 1;\r\nb", Some("rust")), 13);
    }

    #[test]
    fn code_without_a_syntax_is_not_highlighted() {
        for language in [None, Some("text"), Some("TXT"), Some("output"), Some("rsut")] {
            assert_eq!(highlighted_len("a\n", language), 0, "{language:?}");
        }
    }

    #[test]
    fn highlighting_stops_at_the_first_line_over_the_limit() {
        let at_limit = format!("{}\r\n", "a".repeat(MAX_LINE_BYTES));
        let code = format!("x\n{at_limit}{}\ny\n", "a".repeat(MAX_LINE_BYTES + 1));

        assert_eq!(highlighted_len(&code, Some("js")), 2 + at_limit.len());
    }

    #[test]
    fn each_block_that_fits_what_is_left_is_admitted_in_reading_order() {
        let half = MAX_DOCUMENT_BYTES / 2;
        let allowance = Allowance::new([(1, half), (2, half + 1), (3, half), (4, 1)]);

        assert_eq!([1, 2, 3, 4].map(|key| allowance.highlighted(key)), [half, 0, half, 0]);
    }
}
